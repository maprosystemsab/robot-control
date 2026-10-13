/*
    Copyright 2016 Benjamin Vedder	benjamin@vedder.se

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

#ifndef CARCLIENT_H
#define CARCLIENT_H

#include <QObject>
#include <QSerialPort>
#include <QTcpSocket>
#include <QTimer>
#include <QUdpSocket>
#include <QFile>
#include <QProcess>
#include <QImage>
#include <QLocalServer>
#include <QElapsedTimer>
#include "packetinterface.h"
#include "tcpbroadcast.h"
//#include <QSerialPort>
#include "serialport.h"
#include <serialport.h>
#include "ublox.h"
#include "tcpserversimple.h"
#include "rtcmclient.h"
#include "carsim/carsim.h"
#include "nodstopp.h"

#include <QCoreApplication>
#include <QProcess>
#include <QDebug>
#include <thread>
#include <chrono>

class CarClient : public QObject
{
    Q_OBJECT
public:
    static CarClient* currentMsgHandler;
    static rtcm3_state rtcmState;

    typedef struct {
        bool serialConnect;
        QString serialPort;
        int serialBaud;
        bool serialRtcmConnect;
        QString serialRtcmPort;
        int serialRtcmBaud;
        bool serialArduinoConnect;
        QString serialArduinoPort;
        int serialArduinoBaud;
        bool nmeaConnect;
        QString nmeaServer;
        int nmeaPort;
    } settings_t;

    explicit CarClient(QObject *parent = 0);
    ~CarClient();
    void connectSerial(QString port, int baudrate = 115200);
    void connectSerialRtcm(QString port, int baudrate = 9600);
    void connectSerialArduino(QString port, int baudrate = 9600);
    void startRtcmServer(int port = 8200);
    void startUbxServer(int port = 8210);
    void startLogServer(int port = 8410);
    void connectNmea(QString server, int port = 2948);
    void startUdpServer(int port = 8300);
    bool startTcpServer(int port = 8300, QHostAddress addr = QHostAddress::Any);
    bool enableLogging(QString directory);
    void logStop();
    void rtcmRx(QByteArray data, int type);
    void restartRtklib();
    void configureUblox();
    Q_INVOKABLE PacketInterface* packetInterface();
    bool isRtklibRunning();
    quint8 carId();
    void setCarId(quint8 id);
    void connectNtrip(QString server, QString stream, QString user = "", QString pass = "", int port = 80);
    void setSendRtcmBasePos(bool send, double lat = 0.0, double lon = 0.0, double height = 0.0);
    Q_INVOKABLE void rebootSystem(bool powerOff = false);
    Q_INVOKABLE QVariantList getNetworkAddresses();
    Q_INVOKABLE int getBatteryCells();
    void setBatteryCells(int cells);
    void addSimulatedCar(int id);
    CarSim *getSimulatedCar(int id);

    int getCarIdToSet() const;
    void setCarIdToSet(int carIdToSet);

signals:

public slots:
    void serialDataAvailable();
    void serialPortError(int error);
    void serialArduinoDataAvailable();
    void serialRtcmDataAvailable();
    void serialArduinoPortError(QSerialPort::SerialPortError error);
    void serialRtcmPortError(QSerialPort::SerialPortError error);
    void packetDataToSend(QByteArray &data);
    void tcpDataAvailable();
    void tcpConnected();
    void tcpDisconnected();
    void rtcmUsbRx(quint8 id, QByteArray data);
    void reconnectTimerSlot();
    void logFlushTimerSlot();
    void readPendingDatagrams();
    void carPacketRx(quint8 id, CMD_PACKET cmd, const QByteArray &data);
    void logLineUsbReceived(quint8 id, QString str);
    void systemTimeReceived(quint8 id, qint32 sec, qint32 usec);
    void rebootSystemReceived(quint8 id, bool powerOff);
    void ubxRx(const QByteArray &data);
    void rxRawx(ubx_rxm_rawx rawx);
    void tcpRx(QByteArray &data);
    void tcpConnectionChanged(bool connected, QString address);
    void rtcmReceived(QByteArray data, int type, bool sync = false);
    void logEthernetReceived(quint8 id, QByteArray data);

    void handleRos2Connection();
    void readRos2Command();

    // Nödstopp: NC-brytare på GPIO<gpio> (se nodstopp.h). Av om den inte anropas.
    void enableNodstopp(int gpio);


private slots:
    void processCarData(QByteArray data);
    void cameraImageCaptured(QImage img);
    void logBroadcasterDataReceived(QByteArray &data);
    void nodstoppTick();
//    QString processCommand(QByteArray &data);

private:
    PacketInterface *mPacketInterface;
    TcpBroadcast *mRtcmBroadcaster;
    TcpBroadcast *mUbxBroadcaster;
    TcpBroadcast *mLogBroadcaster;
    SerialPort *mSerialPort;
    QSerialPort *mSerialPortRtcm;
    QSerialPort *mSerialPortArduino;
    QTcpSocket *mTcpSocket;
    TcpServerSimple *mTcpServer;
    QLocalServer* ros2Server;
    RtcmClient *mRtcmClient;
    int mCarId;
    QTimer *mReconnectTimer;
    QTimer *mLogFlushTimer;
    settings_t mSettings;
    bool mTcpConnected;
    QUdpSocket *mUdpSocket;
    QHostAddress mHostAddress;
    int mUdpPort;
    QFile mLog;
    Ublox *mUblox;
    bool mRtklibRunning;
    QString mUbloxDevice; // Riktig tty bakom /dev/ublox när porten öppnades (t.ex. /dev/ttyACM1)
    QElapsedTimer mRawxTimer;      // tid sedan senaste RAWX från u-bloxen
    QElapsedTimer mUbloxCfgTimer;  // tid sedan u-bloxen senast fick sina inställningar
    int mUbloxCfgRetries = 0;      // omkonfigureringar i rad utan att RAWX kommit tillbaka
    int mBatteryCells;
    QList<CarSim*> mSimulatedCars;
    QVector<UWB_ANCHOR> mUwbAnchorsNow;
    int mCarIdToSet;

    // Nödstopp (se enableNodstopp). Läget skrivs till /tmp/nodstopp_status för robotd
    // och statusskärmen: "ok", "intryckt", "sparrad" (knappen ute, väntar på ett
    // stoppkommando från föraren) eller "fel" (pinnen går inte att läsa = nödstopp).
    enum NodstoppLage { NS_AV, NS_OK, NS_INTRYCKT, NS_SPARRAD, NS_FEL };
    NodstoppGpio *mNodstopp = nullptr;
    QTimer *mNodstoppTimer = nullptr;
    NodstoppLage mNsLage = NS_AV;
    int mNsBrutenIRad = 0;
    QElapsedTimer mNsSlutenSedan;   // tid som kontakten varit sluten utan avbrott
    QElapsedTimer mNsStoppSkickat;  // senaste nollkommando till kortet
    QElapsedTimer mNsApSkickat;     // senaste "autopilot av" till kortet
    QElapsedTimer mNsFilSkriven;
    bool nodstoppSparrar(const QByteArray &data);
    void nodstoppSattLage(NodstoppLage lage, const QString &text);
    void nodstoppSkrivFil();
    void nodstoppSkickaLage();
    void skickaTillKlienter(const QByteArray &data);

    QString mUsr;
    QString mPwd;

    double mRtcmBaseLat;
    double mRtcmBaseLon;
    double mRtcmBaseHeight;
    bool mRtcmSendBase;
    QString mLogBroadcasterDataBuffer;
    bool mOverrideUwbPos;
    double mOverrideUwbX;
    double mOverrideUwbY;

    bool setUnixTime(qint64 t);
    void printTerminal(QString str);
    void printLog(QString str);
    bool waitProcess(QProcess &process, int timeoutMs = 300000);

    void startStr2Str(double lat,double lon);
    void stopStr2Str();
    QProcess s2sProcess;
};


#endif // CARCLIENT_H´
