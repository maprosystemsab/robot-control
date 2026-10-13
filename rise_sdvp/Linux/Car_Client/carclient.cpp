/*
    Copyright 2016-2017 Benjamin Vedder	benjamin@vedder.se

    This program is free software: you can redistribute it and/or modify
    it under the terms of the GNU General Public License as published by
    the Free Software Foundation, either version 3 of the License, or
    (at your option) any later version.

    This program is distributed in the hope that it will be useful,
    but WITHOUT ANY WARRANTY; without even the implied warranty of
    MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
    GNU General Public License for more details.

    You should have received a copy of the GNU General Public License
    along with this program.  If not, see <http://www.gnu.org/licenses/>.
    */

#include "carclient.h"
#include <QFileInfo>
#include <QDebug>
#include <QDateTime>
#include <cstdio>
#include <QDir>
#include <sys/time.h>
#include <sys/reboot.h>
#include <unistd.h>
#include <QEventLoop>
#include <QCoreApplication>
#include <QNetworkInterface>
#include <QBuffer>
#include <QLocalSocket>
#include "rtcm3_simple.h"
#include "utility.h"
#include "datatypes.h"
#include <cstring>

#include <QCoreApplication>
#include <QProcess>
#include <QDebug>
#include <thread>
#include <chrono>

#define JETSON____

namespace {
/**
 * Callback function for RTCM data reception.
 * Converts raw RTCM data to QByteArray and forwards to CarClient instance.
 * 
 * @param data Pointer to RTCM data buffer
 * @param len Length of data in bytes
 * @param type RTCM message type
 */
void rtcm_rx(uint8_t *data, int len, int type) {
    if (CarClient::currentMsgHandler) {
        QByteArray rtcm_data((const char*)data, len);
        CarClient::currentMsgHandler->rtcmRx(rtcm_data, type);
    }
}
}

// Static member initialization
CarClient *CarClient::currentMsgHandler = 0;
rtcm3_state CarClient::rtcmState;

/**
 * Constructor for CarClient.
 * Initializes all communication interfaces, timers, and signal-slot connections.
 * 
 * @param parent Parent QObject
 */
CarClient::CarClient(QObject *parent) : QObject(parent)
{
    // Initialize settings
    mSettings.nmeaConnect = false;
    mSettings.serialConnect = false;
    mSettings.serialRtcmConnect = false;

    // Create communication interfaces
    mSerialPort = new SerialPort(this);
    mSerialPortRtcm = new QSerialPort(this);
    mSerialPortArduino = new QSerialPort(this);
    mPacketInterface = new PacketInterface(this);
    mRtcmBroadcaster = new TcpBroadcast(this);
    mUbxBroadcaster = new TcpBroadcast(this);
    mLogBroadcaster = new TcpBroadcast(this);
    mUblox = new Ublox(this);
    mTcpSocket = new QTcpSocket(this);
    mTcpServer = new TcpServerSimple(this);

    mRtcmClient = new RtcmClient(this);
    
    // Initialize state variables
    mCarId = 255;  // 255 = undefined
    mReconnectTimer = new QTimer(this);
    mReconnectTimer->start(2000);  // 2 second reconnect interval
    mTcpConnected = false;
    mLogFlushTimer = new QTimer(this);
    mLogFlushTimer->start(2000);  // 2 second log flush interval
    mRtklibRunning = false;
    mBatteryCells = 10;  // Default: 10S battery
    mCarIdToSet = -1;  // No pending ID change
    mOverrideUwbPos = false;
    mOverrideUwbX = 0.0;
    mOverrideUwbY = 0.0;

    // Network configuration
    mHostAddress = QHostAddress("0.0.0.0");
    mUdpPort = 0;
    mUdpSocket = new QUdpSocket(this);

    // RTCM configuration
    mRtcmBaseLat = 0.0;
    mRtcmBaseLon = 0.0;
    mRtcmBaseHeight = 0.0;
    mRtcmSendBase = false;

    // Set up RTCM callback and initialize state
    currentMsgHandler = this;
    rtcm3_init_state(&rtcmState);
    rtcm3_set_rx_callback(rtcm_rx, &rtcmState);

    // Configure TCP server to use packet protocol
    mTcpServer->setUsePacket(true);

    connect(mSerialPort, SIGNAL(serial_data_available()),
            this, SLOT(serialDataAvailable()));
    connect(mSerialPort, SIGNAL(serial_port_error(int)),
            this, SLOT(serialPortError(int)));
    connect(mSerialPortRtcm, SIGNAL(readyRead()),
            this, SLOT(serialRtcmDataAvailable()));
    connect(mSerialPortArduino, SIGNAL(readyRead()),
            this, SLOT(serialArduinoDataAvailable()));
    connect(mTcpSocket, SIGNAL(readyRead()), this, SLOT(tcpDataAvailable()));
    connect(mTcpSocket, SIGNAL(connected()), this, SLOT(tcpConnected()));
    connect(mTcpSocket, SIGNAL(disconnected()), this, SLOT(tcpDisconnected()));
    connect(mPacketInterface, SIGNAL(rtcmUsbReceived(quint8,QByteArray)),
            this, SLOT(rtcmUsbRx(quint8,QByteArray)));
    connect(mPacketInterface, SIGNAL(dataToSend(QByteArray&)),
            this, SLOT(packetDataToSend(QByteArray&)));
    connect(mReconnectTimer, SIGNAL(timeout()),
            this, SLOT(reconnectTimerSlot()));
    connect(mUdpSocket, SIGNAL(readyRead()),
            this, SLOT(readPendingDatagrams()));
    connect(mPacketInterface, SIGNAL(packetReceived(quint8,CMD_PACKET,QByteArray)),
            this, SLOT(carPacketRx(quint8,CMD_PACKET,QByteArray)));
    connect(mPacketInterface, SIGNAL(logLineUsbReceived(quint8,QString)),
            this, SLOT(logLineUsbReceived(quint8,QString)));
    connect(mLogFlushTimer, SIGNAL(timeout()),
            this, SLOT(logFlushTimerSlot()));
    connect(mPacketInterface, SIGNAL(systemTimeReceived(quint8,qint32,qint32)),
            this, SLOT(systemTimeReceived(quint8,qint32,qint32)));
    connect(mPacketInterface, SIGNAL(rebootSystemReceived(quint8,bool)),
            this, SLOT(rebootSystemReceived(quint8,bool)));
    connect(mUblox, SIGNAL(ubxRx(QByteArray)), this, SLOT(ubxRx(QByteArray)));
    connect(mUblox, SIGNAL(rxRawx(ubx_rxm_rawx)), this, SLOT(rxRawx(ubx_rxm_rawx)));
    connect(mTcpServer->packet(), SIGNAL(packetReceived(QByteArray&)),
            this, SLOT(tcpRx(QByteArray&)));
    connect(mTcpServer, SIGNAL(connectionChanged(bool,QString)),
            this, SLOT(tcpConnectionChanged(bool,QString)));
    connect(mRtcmClient, SIGNAL(rtcmReceived(QByteArray,int,bool)),
            this, SLOT(rtcmReceived(QByteArray,int,bool)));
    connect(mPacketInterface, SIGNAL(logEthernetReceived(quint8,QByteArray)),
            this, SLOT(logEthernetReceived(quint8,QByteArray)));
    connect(mLogBroadcaster, SIGNAL(dataReceived(QByteArray&)),
            this, SLOT(logBroadcasterDataReceived(QByteArray&)));

    // Set up ROS 2 local server for inter-process communication
    ros2Server = new QLocalServer(this);
    connect(ros2Server, &QLocalServer::newConnection, this, &CarClient::handleRos2Connection);

    if (!ros2Server->listen("ros2_carclient_channel")) {
        qDebug() << "Unable to start the ROS 2 local server:" << ros2Server->errorString();
        // Handle error - continue without ROS 2 support
    }
    
    // Log startup message
    logLineUsbReceived(0, "All Started!");
}

/**
 * Destructor for CarClient.
 * Ensures proper cleanup by stopping logging before destruction.
 */
CarClient::~CarClient()
{
    logStop();
}

/**
 * Handle new ROS 2 connection.
 * Sets up signal-slot connections for the new client connection.
 */
void CarClient::handleRos2Connection() {
    QLocalSocket* clientConnection = ros2Server->nextPendingConnection();
    connect(clientConnection, &QLocalSocket::readyRead, this, &CarClient::readRos2Command);
    connect(clientConnection, &QLocalSocket::disconnected, clientConnection, &QLocalSocket::deleteLater);
}

/**
 * Connect to controller card via serial port.
 * 
 * @param port Serial port name (e.g., "/dev/ttyUSB0")
 * @param baudrate Baud rate (default: 115200)
 */
void CarClient::connectSerial(QString port, int baudrate)
{
    qDebug() << "Trying to connect to serial port: " << port;
    
    // Close existing connection if open
    if(mSerialPort->isOpen()) {
        mSerialPort->closePort();
    }

    // Open new connection
    mSerialPort->openPort(port, baudrate);

    // Update settings
    mSettings.serialConnect = true;
    mSettings.serialPort = port;
    mSettings.serialBaud = baudrate;

    // Check if connection succeeded
    if(!mSerialPort->isOpen()) {
//        qDebug() << "Serial port connection failed";
        return;
    }

    qDebug() << "Serial port connected";

    mPacketInterface->stopUdpConnection();
    mPacketInterface->getState(255); // To get car ID
}

/**
 * Connect to RTCM GPS receiver via serial port.
 * 
 * @param port Serial port name (e.g., "/dev/ttyUSB1")
 * @param baudrate Baud rate (default: 9600)
 */
void CarClient::connectSerialRtcm(QString port, int baudrate)
{
    // Close existing connection if open
    if(mSerialPortRtcm->isOpen()) {
        mSerialPortRtcm->close();
    }

    // Configure and open new connection
    mSerialPortRtcm->setPortName(port);
    mSerialPortRtcm->open(QIODevice::ReadWrite);

    // Update settings
    mSettings.serialRtcmConnect = true;
    mSettings.serialRtcmPort = port;
    mSettings.serialRtcmBaud = baudrate;

    // Check if connection succeeded
    if(mSerialPortRtcm->isOpen())
    {
        // Tell RC_Controller about it
    } else {
        return;
    }

    qDebug() << "Serial port RTCM connected";

    // Configure serial port parameters
    mSerialPortRtcm->setBaudRate(baudrate);
    mSerialPortRtcm->setDataBits(QSerialPort::Data8);
    mSerialPortRtcm->setParity(QSerialPort::NoParity);
    mSerialPortRtcm->setStopBits(QSerialPort::OneStop);
    mSerialPortRtcm->setFlowControl(QSerialPort::NoFlowControl);
}

/**
 * Connect to Arduino via serial port.
 * 
 * @param port Serial port name (e.g., "/dev/ttyACM0")
 * @param baudrate Baud rate (default: 9600)
 */
void CarClient::connectSerialArduino(QString port, int baudrate)
{
    // Close existing connection if open
    if(mSerialPortArduino->isOpen()) {
        mSerialPortArduino->close();
    }

    // Configure and open new connection
    mSerialPortArduino->setPortName(port);
    mSerialPortArduino->open(QIODevice::ReadWrite);

    // Update settings
    mSettings.serialArduinoConnect = true;
    mSettings.serialArduinoPort = port;
    mSettings.serialArduinoBaud = baudrate;

    // Check if connection succeeded and notify controller
    if(mSerialPortArduino->isOpen())
        {
        // Notify controller that Arduino is connected
        QByteArray packet;
        packet.clear();
        packet.append(this->mCarId);
        packet.append((char)CMD_ARDUINO_STATUS);
        packet.append((char)1);
        mPacketInterface->sendPacket(packet);

            // Tell RC_Controller about it
        } else
        {
        // Notify controller that Arduino connection failed
        QByteArray packet;
        packet.clear();
        packet.append(this->mCarId);
        packet.append((char)CMD_ARDUINO_STATUS);
        packet.append((char)0);
        mPacketInterface->sendPacket(packet);
        return;
        }

    qDebug() << "Serial port Arduino connected";
    qDebug() << "Baudrate: " << baudrate;

    // Configure serial port parameters
    mSerialPortArduino->setBaudRate(baudrate);
    mSerialPortArduino->setDataBits(QSerialPort::Data8);
    mSerialPortArduino->setParity(QSerialPort::NoParity);
    mSerialPortArduino->setStopBits(QSerialPort::OneStop);
    mSerialPortArduino->setFlowControl(QSerialPort::NoFlowControl);
}

/**
 * Start RTCM broadcast server.
 * 
 * @param port TCP port to listen on (default: 8200)
 */
void CarClient::startRtcmServer(int port)
{
    mRtcmBroadcaster->startTcpServer(port);
}

/**
 * Start UBX broadcast server.
 * 
 * @param port TCP port to listen on (default: 8210)
 */
void CarClient::startUbxServer(int port)
{
    mUbxBroadcaster->startTcpServer(port);
}

/**
 * Start log broadcast server.
 * 
 * @param port TCP port to listen on (default: 8410)
 */
void CarClient::startLogServer(int port)
{
    mLogBroadcaster->startTcpServer(port);
}

/**
 * Connect to NMEA server for GPS data.
 * 
 * @param server Server address or hostname
 * @param port TCP port (default: 2948)
 */
void CarClient::connectNmea(QString server, int port)
{
    // Close existing connection
    mTcpSocket->close();
    
    // Connect to NMEA server
    mTcpSocket->connectToHost(server, port);

    // Update settings
    mSettings.nmeaConnect = true;
    mSettings.nmeaServer = server;
    mSettings.nmeaPort = port;
}

/**
 * Start UDP server for broadcast messages.
 * 
 * @param port UDP port to listen on (default: 8300)
 */
void CarClient::startUdpServer(int port)
{
    mUdpPort = port + 1;
    mUdpSocket->close();
    mUdpSocket->bind(QHostAddress::Any, port);
}

bool CarClient::startTcpServer(int port, QHostAddress addr)
{
    qDebug() << "Trying to start TCP server. Port: " << port << ", addr: " << addr;
    bool res = mTcpServer->startServer(port,addr);

    if (!res) {
        qDebug() << "Starting TCP server failed:" << mTcpServer->errorString();
    } else
    {
        qDebug() << "Started :-)";
    }

    return res;
}

/**
 * Enable logging to file.
 * 
 * @param directory Directory to store log files
 * @return true if logging started successfully, false otherwise
 */
bool CarClient::enableLogging(QString directory)
{
    // Close existing log if open
    if (mLog.isOpen()) {
        mLog.close();
    }

    // Generate timestamped log filename
    QString name = QDateTime::currentDateTime().
            toString("LOG_yyyy-MM-dd_hh.mm.ss.log");

    // Create directory if it doesn't exist
    QDir dir;
    dir.mkpath(directory);

    // Set log file path and open
    mLog.setFileName(directory + "/" + name);
    return mLog.open(QIODevice::ReadWrite | QIODevice::Truncate);
}

/**
 * Stop logging and close log file.
 */
void CarClient::logStop()
{
    // Close log file if open
    if (mLog.isOpen()) {
        qDebug() << "Closing log:" << mLog.fileName();
        mLog.close();
    } else {
        qDebug() << "Log not open";
    }
}

void CarClient::rtcmRx(QByteArray data, int type)
{
    (void)type;

    QString str;

    // Print the packet in the car terminal. NOTE: This is for debugging
    for (int i = 0;i < data.size();i++) {
        str.append(QString().asprintf("%02X ", data.at(i)));
//        str.append(QString().sprintf("%02X ", data.at(i)));
        if (i >= 50) {
            break;
        }
    }

    printTerminal(str);
}

void CarClient::configureUblox()
{
    // Inställningarna ligger bara i u-bloxens RAM och försvinner när den startar om
    // (t.ex. när styrkortet flashas eller återställs), därför kan de skickas igen.
    // Serial port baud rate
    // if it is too low the buffer will overfill and it won't work properly.
    ubx_cfg_prt_uart uart;
    uart.baudrate = 115200;
    uart.in_ubx = true;
    uart.in_nmea = true;
    uart.in_rtcm2 = false;
    uart.in_rtcm3 = true;
    uart.out_ubx = true;
    uart.out_nmea = true;
    uart.out_rtcm3 = true;
    mUblox->ubxCfgPrtUart(&uart);

    // Set configuration
    // Switch on RAWX and NMEA messages, set rate to 1 Hz and time reference to UTC
    mUblox->ubxCfgRate(200, 1, 0);
    mUblox->ubxCfgMsg(UBX_CLASS_RXM, UBX_RXM_RAWX, 1); // Every second
    mUblox->ubxCfgMsg(UBX_CLASS_RXM, UBX_RXM_SFRBX, 1); // Every second
    mUblox->ubxCfgMsg(UBX_CLASS_NMEA, UBX_NMEA_GGA, 1); // Every second

    // Automotive dynamic model
    ubx_cfg_nav5 nav5;
    memset(&nav5, 0, sizeof(ubx_cfg_nav5));
    nav5.apply_dyn = true;
    nav5.dyn_model = 4;
    mUblox->ubxCfgNav5(&nav5);

    // Time pulse configuration
    ubx_cfg_tp5 tp5;
    memset(&tp5, 0, sizeof(ubx_cfg_tp5));
    tp5.active = true;
    tp5.polarity = true;
    tp5.alignToTow = true;
    tp5.lockGnssFreq = true;
    tp5.lockedOtherSet = true;
    tp5.syncMode = false;
    tp5.isFreq = false;
    tp5.isLength = true;
    tp5.freq_period = 1000000;
    tp5.pulse_len_ratio = 0;
    tp5.freq_period_lock = 1000000;
    tp5.pulse_len_ratio_lock = 100000;
    tp5.gridUtcGnss = 0;
    tp5.user_config_delay = 0;
    tp5.rf_group_delay = 0;
    tp5.ant_cable_delay = 50;
    mUblox->ubloxCfgTp5(&tp5);
}

void CarClient::restartRtklib()
{
    QFile ublox("/dev/ublox");
    if (!ublox.exists()) {
        mRtklibRunning = false;
        return;
    }

    mRtklibRunning = true;

    mUblox->disconnectSerial();

    if (mUblox->connectSerial(ublox.fileName())) {
        mUbloxDevice = QFileInfo(ublox.fileName()).canonicalFilePath();
        configureUblox();
        mRawxTimer.start();
        mUbloxCfgTimer.start();
        mUbloxCfgRetries = 0;
    }

    QString user = qgetenv("USER");
    if (user.isEmpty()) {
        user = qgetenv("USERNAME");
    }

    if (user.isEmpty()) {
        QProcess process;
        process.setEnvironment(QProcess::systemEnvironment());
        process.start("whoami");
        waitProcess(process);
        user = QString(process.readAllStandardOutput());
        user.replace("\n", "");
    }

    QProcess process;
    process.setEnvironment(QProcess::systemEnvironment());
    process.start("screen", QStringList() <<
                  "-X" << "-S" << "rtklib" << "quit");
    waitProcess(process);

    QProcess process2;
    process2.setEnvironment(QProcess::systemEnvironment());
    process2.start("killall", QStringList() << "rtkrcv");
    waitProcess(process2);

    QProcess process3;
    process3.setEnvironment(QProcess::systemEnvironment());
    // rtkrcv-mappen ligger bredvid Car_Client i samma repo (Linux/Car_Client ->
    // Linux/PI/rtkrcv_arm), var repot än är klonat. De hårdkodade sökvägarna nedan
    // fanns inte på nya installationer (t.ex. ~/rise_sdvp med Linux/PI, inte Linux/RTK),
    // så rtklib startade aldrig och ingen position kom på port 2948.
    QStringList rtkCandidates;
    rtkCandidates << QDir(QCoreApplication::applicationDirPath() + "/../PI/rtkrcv_arm").absolutePath()
                  << QString("/home/%1/RControllStation/rise_sdvp/Linux/PI/rtkrcv_arm").arg(user)
                  << QString("/home/%1/rise_sdvp/Linux/PI/rtkrcv_arm").arg(user)
                  << QString("/home/%1/rise_sdvp/Linux/RTK/rtkrcv_arm").arg(user);
    QString rtkPath = rtkCandidates.first();
    for (const QString &c : rtkCandidates) {
        if (QFile::exists(c + "/start_ublox")) {
            rtkPath = c;
            break;
        }
    }
    qDebug() << "Starting rtklib in" << rtkPath;
    process3.start("screen", QStringList() <<
                   "-d" << "-m" << "-S" << "rtklib" << "bash" << "-c" <<
                   QString("cd %1 && ./start_ublox ; bash").arg(rtkPath));
    waitProcess(process3);
}

PacketInterface *CarClient::packetInterface()
{
    return mPacketInterface;
}

bool CarClient::isRtklibRunning()
{
    return mRtklibRunning;
}

quint8 CarClient::carId()
{
    return mCarId;
}

void CarClient::setCarId(quint8 id)
{
    mCarId = id;
}

void CarClient::connectNtrip(QString server, QString stream, QString user, QString pass, int port)
{
    mRtcmClient->connectNtrip(server, stream, user, pass, port);
}

void CarClient::setSendRtcmBasePos(bool send, double lat, double lon, double height)
{
    mRtcmSendBase = send;
    mRtcmBaseLat = lat;
    mRtcmBaseLon = lon;
    mRtcmBaseHeight = height;
}

void CarClient::rebootSystem(bool powerOff)
{
    // https://askubuntu.com/questions/159007/how-do-i-run-specific-sudo-commands-without-a-password
    QStringList args;
    QString cmd = "sudo";

    if (powerOff) {
        args << "shutdown" << "-h" << "now";
    } else {
        args << "reboot";
    }

    QProcess process;
    process.setEnvironment(QProcess::systemEnvironment());
    process.start(cmd, args);
    waitProcess(process);

    // Avsluta bara om omstarten/avstängningen faktiskt startade. Kräver sudo
    // lösenord (eller saknas rättighet) misslyckas kommandot, och då ska
    // Car_Client fortsätta köra – annars står roboten utan Car_Client tills
    // någon startar om tjänsten för hand (hände på RobAnt 3 2026-10-01).
    if (process.exitStatus() != QProcess::NormalExit || process.exitCode() != 0) {
        qWarning() << (powerOff ? "Shutdown" : "Reboot") << "failed (exit code"
                   << process.exitCode() << "), Car_Client keeps running:"
                   << process.readAllStandardError().trimmed();
        return;
    }

    qApp->quit();
}

QVariantList CarClient::getNetworkAddresses()
{
    QVariantList res;

    for(QHostAddress a: QNetworkInterface::allAddresses()) {
        if(!a.isLoopback()) {
            if (a.protocol() == QAbstractSocket::IPv4Protocol) {
                res << a.toString();
            }
        }
    }

    return res;
}

int CarClient::getBatteryCells()
{
    return mBatteryCells;
}

void CarClient::setBatteryCells(int cells)
{
    mBatteryCells = cells;
}

void CarClient::addSimulatedCar(int id)
{
    CarSim *car = new CarSim(this);
    car->setId(id);
    connect(car, SIGNAL(dataToSend(QByteArray)), this, SLOT(processCarData(QByteArray)));
    mSimulatedCars.append(car);
}

CarSim *CarClient::getSimulatedCar(int id)
{
    CarSim *sim = 0;

    for (CarSim *s: mSimulatedCars) {
        if (s->id() == id) {
            sim = s;
            break;
        }
    }

    return sim;
}

void CarClient::serialDataAvailable()
{
    while (mSerialPort->bytesAvailable() > 0) {
        processCarData(mSerialPort->readAll());
    }
}

void CarClient::serialPortError(int error)
{
    qDebug() << "Serial error:" << error;

    if(mSerialPort->isOpen()) {
        mSerialPort->closePort();
    }
}

void CarClient::serialRtcmDataAvailable()
{
    while (mSerialPortRtcm->bytesAvailable() > 0) {
        QByteArray data = mSerialPortRtcm->readAll();
        for (int i = 0;i < data.size();i++) {
            rtcm3_input_data((uint8_t)data.at(i), &rtcmState);
        }
    }
}

void CarClient::readRos2Command() {
    QLocalSocket* clientConnection = qobject_cast<QLocalSocket*>(sender());
    QByteArray rosdata = clientConnection->readAll();

    rosdata.prepend('\0');
    rosdata.append('\0');
    rosdata.append('\0');
    rosdata.append('\0');
    qDebug() << "Got command 0 via ROS:" << static_cast<unsigned char>(rosdata[0]);
    qDebug() << "Got command 1 via ROS:" << static_cast<unsigned char>(rosdata[1]);
    qDebug() << "Got command 2 via ROS:" << rosdata[2];
    qDebug() << "Got command via ROS:" << rosdata;

    packetDataToSend(rosdata);

    QString response = "OK";

    qDebug() << "Response to ROS: " << response;
    // Send response back to ROS 2 node
    clientConnection->write(response.toUtf8());
    clientConnection->flush();
}


void CarClient::serialArduinoDataAvailable()
{
#define ARDUINO_INTEGER

#ifdef ARDUINO_INTEGER
    while (mSerialPortArduino->bytesAvailable() >= 2) {
        QByteArray data = mSerialPortArduino->read(2);
        if (data.size() == 2) {
                QByteArray packet;
                packet.clear();
                packet.append(this->mCarId);
                packet.append((char)CMD_GETANGLE);
                packet.append(data);

                mPacketInterface->sendPacket(packet);
//            mPacketInterface->sendPacket(data);
        }
    }
#endif

#ifdef ARDUINO_FLOAT
    qDebug() << "Receiving floats!!";

    while (static_cast<quint64>(mSerialPortArduino->bytesAvailable()) >= sizeof(float)) {
        QByteArray data = mSerialPortArduino->read(sizeof(float));

        if (data.size() == sizeof(float)) {
            float value;
            memcpy(&value, data.constData(), sizeof(float));
            qDebug() << "Received float:" << value;

            // Create a QByteArray with the marker byte and the float data
            QByteArray message;
            message.append(static_cast<char>(100)); // Add the marker byte
            message.append((const char*)&value, sizeof(float)); // Add the float data

            // Call tcpRx with the modified message
            tcpRx(message);
        } else {
            qDebug() << "Incomplete data received";
        }
    }

/*
    qDebug() << "Receiving floats!!";

while (static_cast<quint64>(mSerialPortArduino->bytesAvailable()) >= sizeof(float)) {
        QByteArray data = mSerialPortArduino->read(sizeof(float));

        if (data.size() == sizeof(float)) {
            float value;
            memcpy(&value, data.constData(), sizeof(float));
            qDebug() << "Received float:" << value;
        } else {
            qDebug() << "Incomplete data received";
        }
    }
*/
#endif

}

void CarClient::serialRtcmPortError(QSerialPort::SerialPortError error)
{
    QString message;
    switch (error) {
    case QSerialPort::NoError:
        break;
    case QSerialPort::DeviceNotFoundError:
        message = tr("Device not found");
        break;
    case QSerialPort::OpenError:
        message = tr("Can't open device");
        break;
    case QSerialPort::NotOpenError:
        message = tr("Not open error");
        break;
    case QSerialPort::ResourceError:
        message = tr("Port disconnected");
        break;
    case QSerialPort::PermissionError:
        message = tr("Permission error");
        break;
    case QSerialPort::UnknownError:
        message = tr("Unknown error");
        break;
    default:
        message = "Error number: " + QString::number(error);
        break;
    }

    if(!message.isEmpty()) {
        qDebug() << "Serial error:" << message;

        if(mSerialPortRtcm->isOpen()) {
            mSerialPortRtcm->close();
        }
    }
}

void CarClient::serialArduinoPortError(QSerialPort::SerialPortError error)
{
    QString message;
    switch (error) {
    case QSerialPort::NoError:
        break;
    case QSerialPort::DeviceNotFoundError:
        message = tr("Device not found");
        break;
    case QSerialPort::OpenError:
        message = tr("Can't open device");
        break;
    case QSerialPort::NotOpenError:
        message = tr("Not open error");
        break;
    case QSerialPort::ResourceError:
        message = tr("Port disconnected");
        break;
    case QSerialPort::PermissionError:
        message = tr("Permission error");
        break;
    case QSerialPort::UnknownError:
        message = tr("Unknown error");
        break;
    default:
        message = "Error number: " + QString::number(error);
        break;
    }

    if(!message.isEmpty()) {
        qDebug() << "Serial error:" << message;

        if(mSerialPortArduino->isOpen()) {
            mSerialPortArduino->close();
        }
    }
}


void CarClient::packetDataToSend(QByteArray &data)
{
    // This is a packet from RControlStation going to the car.
    // Inspect data and possibly process it here.
#ifdef JETSON____
  // qDebug() << "in CarClient::packetDataToSend. Data: " << data;
#else
    qDebug() << "in CarClient::packetDataToSend. Data: " << data << Qt::flush;
#endif
    bool packetConsumed = false;

    VByteArray vb(data);
    vb.remove(0, data[0]);

    quint8 id = vb.vbPopFrontUint8();
    CMD_PACKET cmd = (CMD_PACKET)vb.vbPopFrontUint8();
    vb.chop(3);

    (void)id;
    qDebug() << "in CarClient::packetDataToSend.... Id: " << id << ", mCarId: " << mCarId << ", cmd: " << cmd;

    if (id == mCarId || id == 0 || id == 255 || mCarId == 255) {
        if (cmd == CMD_CAMERA_STREAM_START) {
        } else if (cmd == CMD_TERMINAL_CMD) {
            QString str(vb);
 //           qDebug() << "to terminal:" << str;

            if (str == "help") {
//                qDebug() << "Read HELP";
                printTerminal("lsusb\n"
                              "  Print information about connected usbs.");
                printTerminal("list_ttys\n"
                              "  Print information about connected ttys.");
                printTerminal("is_arduino_online\n"
                              "  Print information about connected arduino.");
            }
            if (str=="lsusb")
            {
                std::string resultstr=utility::systemcmd("lsusb");
                printTerminal(resultstr.c_str());
            }
            if (str=="list_ttys")
            {
                std::string resultstr=utility::systemcmd("ls /dev/tty*");
                printTerminal(resultstr.c_str());
            }
            if (str=="is_arduino_online")
            {
                if(mSerialPortRtcm->isOpen())
                    {
                    printTerminal("Arduino port open!\n");
                    }
            }
        } else if (cmd == CMD_SET_USER) {
            QString str(vb);
            mUsr=str;
            qDebug() << "User:" << mUsr;
            packetConsumed=true;
        } else if (cmd == CMD_SET_PWD) {
            QString str(vb);
            mPwd=str;
            qDebug() << "Pwd:" << mPwd;
            packetConsumed=true;
        } else if (cmd == CMD_SET_ENU_REF) {
//            int id = vb.vbPopFrontInt16();
            double lat = vb.vbPopFrontDouble64(1.0e16);
            double lon = vb.vbPopFrontDouble64(1.0e16);
            qDebug() << "setting ENU ref";
            stopStr2Str();
            startStr2Str(lat,lon);
        } else if (cmd == CMD_CLEAR_UWB_ANCHORS) {
            mUwbAnchorsNow.clear();
        } else if (cmd == CMD_ADD_UWB_ANCHOR) {
            UWB_ANCHOR a;
            a.id = vb.vbPopFrontInt16();
            a.px = vb.vbPopFrontDouble32Auto();
            a.py = vb.vbPopFrontDouble32Auto();
            a.height = vb.vbPopFrontDouble32Auto();
            mUwbAnchorsNow.append(a);
        }
    }

    if (!packetConsumed) {
        if (mSerialPort->isOpen()) {
//            qDebug() << "Packet sent (port open)";
//            qDebug() << static_cast<int>(data[2]) << ":" << static_cast<int>(data[3]) << ":" << static_cast<int>(data[4]);
            mSerialPort->writeData(data);
        } else
        {
            qDebug() << "Packet not sent (port closed)";

        }
    }
}

void CarClient::tcpDataAvailable()
{
    while (!mTcpSocket->atEnd()) {
        QByteArray data = mTcpSocket->readAll();
        // TODO: Collect data and split at newline. (seems ok)
        mPacketInterface->sendNmeaRadio(mCarId, data);
    }
}

void CarClient::tcpConnected()
{
    qDebug() << "NMEA TCP Connected";
    mTcpConnected = true;
}

void CarClient::tcpDisconnected()
{
    qDebug() << "NMEA TCP Disconnected";
    mTcpConnected = false;
}

void CarClient::rtcmUsbRx(quint8 id, QByteArray data)
{
    (void)id;
    mRtcmBroadcaster->broadcastData(data);
}

void CarClient::reconnectTimerSlot()
{
    // Try to reconnect if the connections are lost
    if (mSettings.serialConnect && !mSerialPort->isOpen()) {
 //       qDebug() << "Trying to reconnect serial...";
        connectSerial(mSettings.serialPort, mSettings.serialBaud);
    }

    if (mSettings.serialRtcmConnect && !mSerialPortRtcm->isOpen()) {
//        qDebug() << "Trying to reconnect RTCM serial...";
        connectSerialRtcm(mSettings.serialRtcmPort, mSettings.serialRtcmBaud);
    }

    if (mSettings.serialArduinoConnect && !mSerialPortArduino->isOpen()) {
//        qDebug() << "Trying to reconnect Arduino serial...";
        connectSerialArduino(mSettings.serialArduinoPort, mSettings.serialArduinoBaud);
    }

    // u-blox har tappats (USB-bortfall/strömavbrott): öppna och konfigurera om den
    // när enheten finns igen, annars står rtkrcv utan UBX-data för alltid.
    if (mRtklibRunning && !mUblox->isSerialConnected() && QFile::exists("/dev/ublox")) {
        qDebug() << "u-blox lost, reconnecting and restarting rtklib...";
        restartRtklib();
    } else if (mRtklibRunning && mUblox->isSerialConnected() && QFile::exists("/dev/ublox") &&
               QFileInfo("/dev/ublox").canonicalFilePath() != mUbloxDevice) {
        // u-bloxen har kommit tillbaka under ett nytt tty-namn (t.ex. när styrkortet
        // flashas och USB räknas om). Den gamla porten ser fortfarande öppen ut men
        // pekar på en borttagen enhet, så inget fel signaleras – jämför namnen i stället.
        qDebug() << "u-blox moved from" << mUbloxDevice << "to"
                 << QFileInfo("/dev/ublox").canonicalFilePath() << ", reconnecting and restarting rtklib...";
        restartRtklib();
    } else if (mRtklibRunning && mUblox->isSerialConnected() && mRawxTimer.isValid() &&
               mRawxTimer.elapsed() > 10000 && mUbloxCfgTimer.elapsed() > 10000) {
        // Porten är öppen men ingen RAWX: u-bloxen har startat om (styrkortet flashat
        // eller återställt) och tappat sina inställningar, utan att USB försvann. Skicka
        // inställningarna igen; hjälper inte det på tre försök, öppna porten på nytt.
        if (mUbloxCfgRetries < 3) {
            mUbloxCfgRetries++;
            qDebug() << "No RAWX from u-blox for" << mRawxTimer.elapsed() / 1000
                     << "s, sending configuration again (attempt" << mUbloxCfgRetries << ")";
            configureUblox();
            mUbloxCfgTimer.restart();
        } else {
            qDebug() << "Still no RAWX from u-blox, reconnecting and restarting rtklib...";
            restartRtklib();
        }
    }

    if (mSettings.nmeaConnect && !mTcpConnected) {
        qDebug() << "Trying to reconnect nmea tcp...";
        connectNmea(mSettings.nmeaServer, mSettings.nmeaPort);
    }

    if ((mRtcmClient->isTcpConnected() || mRtcmClient->isSerialConnected()) && mRtcmSendBase) {
        mPacketInterface->sendRtcmUsb(255, mRtcmClient->encodeBasePos(mRtcmBaseLat,
                                                                      mRtcmBaseLon,
                                                                      mRtcmBaseHeight,
                                                                      0.0));
    }

    // Set ID of board on regular basis. In case reprogramming is done.
    if (mSerialPort->isOpen() && mCarIdToSet >= 0 && mCarIdToSet < 255) {
        mPacketInterface->sendTerminalCmd(255, QString("set_id_quiet %1").arg(mCarIdToSet));
    }
}

void CarClient::logFlushTimerSlot()
{
    if (mLog.isOpen()) {
//        mLog.flush();
    }
}

void CarClient::readPendingDatagrams()
{
    while (mUdpSocket->hasPendingDatagrams()) {
        QByteArray datagram;
        datagram.resize(mUdpSocket->pendingDatagramSize());
        quint16 senderPort;

        mUdpSocket->readDatagram(datagram.data(), datagram.size(),
                                &mHostAddress, &senderPort);

        if (nodstoppSparrar(datagram)) {
            continue;
        }
        mPacketInterface->sendPacket(datagram);
    }
}

void CarClient::carPacketRx(quint8 id, CMD_PACKET cmd, const QByteArray &data)
{
    QByteArray toSend = data;

    /* Uncomment as likely to produce confusing errors when the composition and size of data is changed
    if (cmd == CMD_GET_STATE && mOverrideUwbPos) {
        VByteArray vb;
        vb.append(data.mid(0, 99));
        vb.vbAppendDouble32(mOverrideUwbX, 1e4);
        vb.vbAppendDouble32(mOverrideUwbY, 1e4);
        toSend = vb;
    }
*/
    if (id != 254) {
//        qDebug() << "In CarClient::carPacketRx. Car: " << id;
        if (mCarId == 255) {
            mCarId = id;
        }

        if (QString::compare(mHostAddress.toString(), "0.0.0.0") != 0) {
//            qDebug() << "datagramming";
            mUdpSocket->writeDatagram(toSend, mHostAddress, mUdpPort);
        }
//        qDebug() << "tcpservering";
        mTcpServer->packet()->sendPacket(toSend);
    }
}

void CarClient::logLineUsbReceived(quint8 id, QString str)
{
    (void)id;

    if (mLog.isOpen()) {
        qDebug() << "Writing to log: " << str;
        mLog.write(str.toLocal8Bit());
    }
}

void CarClient::systemTimeReceived(quint8 id, qint32 sec, qint32 usec)
{
    (void)id;
    (void)usec;

//    struct timeval now;
//    int rc;
//    now.tv_sec = sec;
//    now.tv_usec = usec;
//    rc = settimeofday(&now, NULL);

    if(setUnixTime(sec)) {
        qDebug() << "Sucessfully set system time";
        restartRtklib();
    } else {
        qDebug() << "Setting system time failed";
    }
}

void CarClient::rebootSystemReceived(quint8 id, bool powerOff)
{
    (void)id;

    logStop();
    sync();

//    if (powerOff) {
//        reboot(RB_POWER_OFF);
//    } else {
//        reboot(RB_AUTOBOOT);
//    }

    rebootSystem(powerOff);
}

void CarClient::ubxRx(const QByteArray &data)
{
    mUbxBroadcaster->broadcastData(data);
}

void CarClient::rxRawx(ubx_rxm_rawx rawx)
{
    mRawxTimer.restart();
    mUbloxCfgRetries = 0;

    if (!rawx.leap_sec || rawx.week < 2000) {
        // Leap seconds or week number are not known yet...
        return;
    }

    QDateTime dateGps(QDate(1980, 1, 6), QTime(0, 0, 0));
    dateGps.setOffsetFromUtc(0);
    dateGps = dateGps.addDays(rawx.week * 7);
    dateGps = dateGps.addMSecs((rawx.rcv_tow - (double)rawx.leaps) * 1000.0);

    QDateTime date = QDateTime::currentDateTime();
    qint64 diff = dateGps.toMSecsSinceEpoch() - date.toMSecsSinceEpoch();

    if (abs(diff) > 60000) {
        // Set the system time if the difference is over 10 seconds.
        qDebug() << "System time is different from GPS time. Difference: " << diff << " ms";

//        struct timeval now;
//        int rc;
//        now.tv_sec = dateGps.toTime_t();
//        now.tv_usec = dateGps.time().msec() * 1000.0;
//        rc = settimeofday(&now, NULL);

//        if(setUnixTime(dateGps.toTime_t())) {
          if(setUnixTime(dateGps.toSecsSinceEpoch())) {
            qDebug() << "Sucessfully updated system time";
            restartRtklib();
        } else {
            qDebug() << "Setting system time failed";
        }
    }
}

void CarClient::tcpRx(QByteArray &data)
{
    qDebug() << "In CarClient::tcpRx";
    qDebug() << "data: " << data;
    if (nodstoppSparrar(data)) {
        return;
    }
    mPacketInterface->sendPacket(data);
}

void CarClient::tcpConnectionChanged(bool connected, QString address)
{
    if (connected) {
        qDebug() << "TCP connection from" << address << "accepted";
    } else {
        qDebug() << "Disconnected TCP from" << address;
    }
}

void CarClient::rtcmReceived(QByteArray data, int type, bool sync)
{
    (void)type;
    (void)sync;
    mPacketInterface->sendRtcmUsb(255, data);
}

void CarClient::logEthernetReceived(quint8 id, QByteArray data)
{
    (void)id;
    mLogBroadcaster->broadcastData(data);
}

void CarClient::processCarData(QByteArray data)
{
    mPacketInterface->processData(data);
}     //   qDebug() << "Published: " << str;

void CarClient::cameraImageCaptured(QImage img)
{
    (void)img;
}

void CarClient::logBroadcasterDataReceived(QByteArray &data)
{
    mLogBroadcasterDataBuffer.append(data);
    int newLineIndex = mLogBroadcasterDataBuffer.indexOf("\n");
    while (newLineIndex >= 0) {
        QString line = mLogBroadcasterDataBuffer.left(newLineIndex);
        mLogBroadcasterDataBuffer.remove(0, newLineIndex + 1);
        QStringList tokens = line.split(" ");
        if (tokens.at(0) == "plot_init") {
            if (tokens.size() == 3) {
                QByteArray data;
                data.append((quint8)mCarId);
                data.append((char)CMD_PLOT_INIT);
                data.append(tokens.at(1).toLocal8Bit());
                data.append('\0');
                data.append(tokens.at(2).toLocal8Bit());
                data.append('\0');
                carPacketRx(mCarId, CMD_PLOT_INIT, data);
            }
        } else if (tokens.at(0) == "plot_add_graph") {
            if (tokens.size() == 2) {
                QByteArray data;
                data.append((quint8)mCarId);
                data.append((char)CMD_PLOT_ADD_GRAPH);
                data.append(tokens.at(1).toLocal8Bit());
                data.append('\0');
                carPacketRx(mCarId, CMD_PLOT_ADD_GRAPH, data);
            }
        } else if (tokens.at(0) == "plot_set_graph") {
            if (tokens.size() == 2) {
                QByteArray data;
                data.append((quint8)mCarId);
                data.append((char)CMD_PLOT_SET_GRAPH);
                data.append(tokens.at(1).toInt());
                data.append('\0');
                carPacketRx(mCarId, CMD_PLOT_SET_GRAPH, data);
            }
        } else if (tokens.at(0) == "plot_add_sample") {
            if (tokens.size() == 3) {
                VByteArray data;
                data.append((quint8)mCarId);
                data.append((char)CMD_PLOT_DATA);
                data.vbAppendDouble32Auto(tokens.at(1).toDouble());
                data.vbAppendDouble32Auto(tokens.at(2).toDouble());
                carPacketRx(mCarId, CMD_PLOT_DATA, data);
            }
        } else if (tokens.at(0) == "uwb_pos_override") {
            if (tokens.size() == 3) {
                mOverrideUwbPos = true;
                mOverrideUwbX = tokens.at(1).toDouble();
                mOverrideUwbY = tokens.at(2).toDouble();
            }
        } else if (tokens.at(0) == "uwb_pos_override_stop") {
            if (tokens.size() == 1) {
                mOverrideUwbPos = false;
            }
        } else if (tokens.at(0) == "get_uwb_anchor_list") {
            QString reply;
            reply.append("anchors");
            for (UWB_ANCHOR a: mUwbAnchorsNow) {
                reply.append(QString(" %1 %2 %3 %4").
                             arg(a.id).
                             arg(a.px, 0, 'f', 3).
                             arg(a.py, 0, 'f', 3).
                             arg(a.height, 0, 'f', 3));
            }
            reply.append("\r\n");
            mLogBroadcaster->broadcastData(reply.toLocal8Bit());
        }
        newLineIndex = mLogBroadcasterDataBuffer.indexOf("\n");
    }
}

int CarClient::getCarIdToSet() const
{
    return mCarIdToSet;
}

/**
 * @brief CarClient::setCarIdToSet
 * Set ID that the connected board will be updated with every 2 seconds.
 *
 * @param carIdToSet
 * The ID.
 */
void CarClient::setCarIdToSet(int carIdToSet)
{
    mCarIdToSet = carIdToSet;
}

bool CarClient::setUnixTime(qint64 t)
{
    // https://askubuntu.com/questions/159007/how-do-i-run-specific-sudo-commands-without-a-password
    QProcess process;
    process.setEnvironment(QProcess::systemEnvironment());
    process.start("sudo", QStringList() << "date" << "+%s" << "-s" << QString("@%1").arg(t));
    return waitProcess(process, 5000);
}

void CarClient::printTerminal(QString str)
{
    QByteArray packet;
    packet.append((quint8)mCarId);
    packet.append((char)CMD_PRINTF);
    packet.append(str.toLocal8Bit());
    carPacketRx(mCarId, CMD_PRINTF, packet);

    // Existing behavior (e.g., print to the terminal)
    // Attempt to connect to the local socket without blocking
    QLocalSocket socket;
    socket.connectToServer("ros2_carclient_terminal_channel");
    if (socket.state() == QLocalSocket::UnconnectedState) {
     //   qDebug() << "No ROS 2 channel available, skipping publication.";
    } else
    {
        if (!socket.waitForConnected(0)) {  // Non-blocking wait
     //   qDebug() << "Failed to connect to ROS 2 channel.";
        } else
        {
        // Publish the message if the server is available
        QByteArray data = str.toUtf8();
        socket.write(data);
        socket.flush();
        socket.disconnectFromServer();
        }
    }
}

void CarClient::printLog(QString str)
{
    QByteArray packet;
    packet.append((quint8)mCarId);
    packet.append((char)CMD_PRINTLOG);
    packet.append(str.toLocal8Bit());
    carPacketRx(mCarId, CMD_PRINTLOG, packet);
}

bool CarClient::waitProcess(QProcess &process, int timeoutMs)
{
    bool killed = false;
    process.waitForStarted();

    QEventLoop loop;
    QTimer timeoutTimer;
    timeoutTimer.setSingleShot(true);
    timeoutTimer.start(timeoutMs);
    connect(&process, SIGNAL(finished(int)), &loop, SLOT(quit()));
    connect(&timeoutTimer, SIGNAL(timeout()), &loop, SLOT(quit()));
    loop.exec();

    if (process.state() == QProcess::Running) {
        process.kill();
        process.waitForFinished();
        killed = true;
    }
    return !killed;
}

void CarClient::startStr2Str(double lat,double lon )
{
    qDebug() << "Lat: " << lat << ", Lon: " << lon;
    QString strLat=QString("%1").arg(lat, 0, 'f', 14);
    QString strLon=QString("%1").arg(lon, 0, 'f', 14);
        QString exec="str2str -in ntrip://"+mUsr+":"+mPwd+"@nrtk-swepos.lm.se:8500/RTCM3_GNSS -p "+strLat+" "+strLon+" 17 -n 5 -out serial://rtk:115200:8:r:1 -msg ""1005,1074""";
    qDebug() << exec;

    QStringList args;
    args << "-in" << QString("ntrip://%1:%2@nrtk-swepos.lm.se:80/RTCM3_GNSS").arg(mUsr, mPwd)
         << "-p" << strLat << strLon << "17"
         << "-n" << "1"
         << "-out" << "serial://rtk:115200:8:n:1"
         << "-msg" << "1005,1074,1084,1094,1230";

    QProcessEnvironment env = QProcessEnvironment::systemEnvironment();
    s2sProcess.setProcessEnvironment(env);

    s2sProcess.setProgram("str2str");
    s2sProcess.setArguments(args);
    s2sProcess.start();

    if (!s2sProcess.waitForStarted()) {
        qCritical() << "Failed to start str2str.";
    } else {
        qDebug() << "Started str2str with PID:" << s2sProcess.processId();
    }
}

void CarClient::stopStr2Str() {
    if (s2sProcess.state() == QProcess::Running) {
        s2sProcess.terminate();
        if (!s2sProcess.waitForFinished(1000)) { // Wait max 1 second!
            qDebug() << "str2str did not stop in time, force-killing it...";
            s2sProcess.kill();
            s2sProcess.waitForFinished(500);
        }
        qDebug() << "Stopped str2str with PID:" << s2sProcess.processId();
    } else {
        qCritical() << "No running instance of str2str to stop.";
    }
}

// ---------------------------------------------------------------------------
// Nödstopp (NC-brytare på en GPIO-pinne, se nodstopp.h)
//
// Alla körkommandon från RControlStation och robotd går genom Car_Client, så spärren
// sitter här. Medan knappen är intryckt:
//   - släpps inga körkommandon igenom (fart, styrning, servo, hydraulik, autopilot på),
//   - skickar Car_Client själv "fart = 0" (aktivitet 10) var 200 ms och "autopilot av"
//     varje sekund till styrkortet (styrkortets CMD_EMERGENCY_STOP gör ingenting).
// När knappen dras ut förblir körningen spärrad tills föraren skickar ett stoppkommando:
// spaken i neutralläge, Lås/AKTIVERA av (Robotstyrning) eller Stopp/Esc (RControlStation).
// Roboten kan alltså aldrig börja köra av sig själv när knappen dras ut.
// ---------------------------------------------------------------------------

namespace {
const quint8 NS_CMD_AP_SET_ACTIVE = 59;
const quint8 NS_CMD_EMERGENCY_STOP = 76;
const quint8 NS_CMD_IO_BOARD_SET_PWM_DUTY = 86;
const quint8 NS_CMD_IO_BOARD_SET_VALVE = 87;
const quint8 NS_CMD_HYDRAULIC_MOVE = 88;
const quint8 NS_CMD_RC_CONTROL = 121;
const quint8 NS_CMD_SET_SERVO_DIRECT = 122;
const quint8 NS_CMD_RC_CONTROL_ADV = 125;
const quint8 NS_CMD_STATE_CONTROL_ENABLE = 132;
const quint8 NS_CMD_STATE_CONTROL_TARGET = 133;
const quint8 NS_CMD_TEST_SPIN_VESC = 142; // provsnurr från Robotstyrning (firmware 30.4)
// Car_Client -> klienterna varje sekund: [id][143][läge], läge 0 ok, 1 intryckt,
// 2 spärrad (utdragen, väntar på Stopp), 3 fel. RControlStation visar det på Stopp-knappen.
const quint8 NS_CMD_NODSTOPP_STATUS = 143;
const quint8 NS_AKTIVITET_FART = 10;
const quint8 NS_RC_MODE_CURRENT_BRAKE = 3;

qint32 nsInt32(const QByteArray &d, int ind)
{
    return (qint32)(((quint32)(quint8)d[ind] << 24) | ((quint32)(quint8)d[ind + 1] << 16) |
                    ((quint32)(quint8)d[ind + 2] << 8) | (quint32)(quint8)d[ind + 3]);
}
}

void CarClient::enableNodstopp(int gpio)
{
    QFile::remove("/tmp/nodstopp_status");
    mNodstopp = new NodstoppGpio();
    QString fel;
    if (mNodstopp->open(gpio, fel)) {
        qWarning() << "Nödstopp: läser GPIO" << gpio << "(NC-brytare mot GND)";
        // Börja spärrat: körning släpps först efter ett stoppkommando från föraren.
        nodstoppSattLage(NS_SPARRAD, "Nödstopp: Car_Client startad, väntar på stoppkommando innan körning tillåts");
    } else {
        nodstoppSattLage(NS_FEL, "Nödstopp: " + fel + " — all körning spärrad");
    }

    mNodstoppTimer = new QTimer(this);
    connect(mNodstoppTimer, SIGNAL(timeout()), this, SLOT(nodstoppTick()));
    mNodstoppTimer->start(20);
}

void CarClient::nodstoppTick()
{
    const int sluten = mNodstopp->lasSluten();

    if (sluten < 0) {
        if (mNsLage != NS_FEL) {
            nodstoppSattLage(NS_FEL, "Nödstopp: kan inte läsa GPIO-pinnen — all körning spärrad");
        }
    } else if (sluten == 0) {
        mNsSlutenSedan.invalidate();
        // Två läsningar i rad (40 ms) så att en enstaka störning inte stoppar roboten.
        if (++mNsBrutenIRad >= 2 && mNsLage != NS_INTRYCKT) {
            nodstoppSattLage(NS_INTRYCKT, "NÖDSTOPP INTRYCKT — körningen stoppad");
        }
    } else {
        mNsBrutenIRad = 0;
        if (!mNsSlutenSedan.isValid()) {
            mNsSlutenSedan.start();
        }
        // Knappen ska ha varit ute i 0,5 s innan spärren kan släppas.
        if ((mNsLage == NS_INTRYCKT || mNsLage == NS_FEL) && mNsSlutenSedan.elapsed() > 500) {
            nodstoppSattLage(NS_SPARRAD, "Nödstopp utdraget. Släpp spaken / tryck Lås eller Stopp innan du kör igen");
        }
    }

    if (mNsLage == NS_INTRYCKT || mNsLage == NS_FEL) {
        if (!mNsStoppSkickat.isValid() || mNsStoppSkickat.elapsed() >= 200) {
            QByteArray fart;
            fart.append((char)255);
            fart.append((char)NS_CMD_RC_CONTROL_ADV);
            fart.append((char)NS_AKTIVITET_FART);
            fart.append(QByteArray(4, 0));
            mPacketInterface->sendPacket(fart);
            mNsStoppSkickat.start();
        }
        if (!mNsApSkickat.isValid() || mNsApSkickat.elapsed() >= 1000) {
            QByteArray ap;
            ap.append((char)255);
            ap.append((char)NS_CMD_AP_SET_ACTIVE);
            ap.append((char)0);
            ap.append((char)0);
            mPacketInterface->sendPacket(ap);
            mNsApSkickat.start();
        }
    }

    if (!mNsFilSkriven.isValid() || mNsFilSkriven.elapsed() >= 1000) {
        nodstoppSkrivFil();
        nodstoppSkickaLage();
    }
}

void CarClient::nodstoppSkickaLage()
{
    quint8 lage = 0;
    switch (mNsLage) {
    case NS_INTRYCKT: lage = 1; break;
    case NS_SPARRAD: lage = 2; break;
    case NS_FEL: lage = 3; break;
    default: break;
    }

    QByteArray pkt;
    pkt.append((char)(mCarId >= 0 ? mCarId : 0));
    pkt.append((char)NS_CMD_NODSTOPP_STATUS);
    pkt.append((char)lage);
    skickaTillKlienter(pkt);
}

bool CarClient::nodstoppSparrar(const QByteArray &data)
{
    if (mNsLage == NS_AV || mNsLage == NS_OK || data.size() < 2) {
        return false;
    }

    const quint8 cmd = (quint8)data[1];
    bool stopp = false;   // kommandot betyder "stå still" och får gå igenom
    bool korning = false; // kommandot kan få roboten att röra sig

    switch (cmd) {
    case NS_CMD_AP_SET_ACTIVE:
        stopp = data.size() >= 3 && data[2] == 0;
        korning = !stopp;
        break;
    case NS_CMD_RC_CONTROL_ADV:
        // Bara "fart = 0" släpps igenom. Styrning och övriga aktiviteter spärras.
        stopp = data.size() >= 7 && (quint8)data[2] == NS_AKTIVITET_FART && nsInt32(data, 3) == 0;
        korning = !stopp;
        break;
    case NS_CMD_RC_CONTROL:
        // RControlStations Stopp skickar broms här. Spärras medan knappen är intryckt
        // (kommandot styr även hjulen rakt), men räknas som stoppkommando efteråt.
        stopp = data.size() >= 7 && ((quint8)data[2] == NS_RC_MODE_CURRENT_BRAKE || nsInt32(data, 3) == 0);
        korning = true;
        break;
    case NS_CMD_EMERGENCY_STOP:
        stopp = true;
        break;
    case NS_CMD_IO_BOARD_SET_PWM_DUTY:
    case NS_CMD_IO_BOARD_SET_VALVE:
    case NS_CMD_HYDRAULIC_MOVE:
    case NS_CMD_SET_SERVO_DIRECT:
    case NS_CMD_STATE_CONTROL_ENABLE:
    case NS_CMD_STATE_CONTROL_TARGET:
    case NS_CMD_TEST_SPIN_VESC:
        korning = true;
        break;
    default:
        break;
    }

    if (mNsLage == NS_SPARRAD && stopp) {
        nodstoppSattLage(NS_OK, "Nödstopp: spärren släppt, körning tillåten");
        return false;
    }

    if (korning) {
        static QElapsedTimer senastLoggat;
        if (!senastLoggat.isValid() || senastLoggat.elapsed() > 1000) {
            qWarning() << "Nödstopp: spärrade kommando" << cmd;
            senastLoggat.start();
        }
    }
    return korning;
}

void CarClient::nodstoppSattLage(NodstoppLage lage, const QString &text)
{
    mNsLage = lage;
    mNsStoppSkickat.invalidate();
    mNsApSkickat.invalidate();
    qWarning() << text;
    nodstoppSkrivFil();
    nodstoppSkickaLage();

    // Visas i RControlStations terminal och i robotd:s logg.
    QByteArray printf;
    printf.append((char)(mCarId >= 0 ? mCarId : 0));
    printf.append((char)CMD_PRINTF);
    printf.append(text.toUtf8());
    skickaTillKlienter(printf);
}

void CarClient::nodstoppSkrivFil()
{
    const char *namn = "ok";
    switch (mNsLage) {
    case NS_INTRYCKT: namn = "intryckt"; break;
    case NS_SPARRAD: namn = "sparrad"; break;
    case NS_FEL: namn = "fel"; break;
    default: break;
    }

    // Skriv till en tillfällig fil och byt namn, så att läsarna aldrig ser en halv fil.
    QFile f("/tmp/nodstopp_status.tmp");
    if (f.open(QIODevice::WriteOnly | QIODevice::Truncate)) {
        f.write(QString("%1 %2\n").arg(namn).arg(QDateTime::currentSecsSinceEpoch()).toLatin1());
        f.close();
        ::rename("/tmp/nodstopp_status.tmp", "/tmp/nodstopp_status");
    }
    mNsFilSkriven.start();
}

void CarClient::skickaTillKlienter(const QByteArray &data)
{
    QByteArray toSend = data;
    if (QString::compare(mHostAddress.toString(), "0.0.0.0") != 0) {
        mUdpSocket->writeDatagram(toSend, mHostAddress, mUdpPort);
    }
    mTcpServer->packet()->sendPacket(toSend);
}
