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

#include "tcpserversimple.h"
#include <QDebug>
#include <QLocalSocket>

#ifdef Q_OS_LINUX
#include <sys/socket.h>
#include <netinet/in.h>
#include <netinet/tcp.h>
#endif

/**
 * Constructor for TcpServerSimple.
 * Initializes TCP server, packet handler, and sets up signal-slot connections.
 * 
 * @param parent Parent QObject
 */
TcpServerSimple::TcpServerSimple(QObject *parent) : QObject(parent)
{
    // Create TCP server
    mTcpServer = new QTcpServer(this);
    
    // Create packet handler
    mPacket = new Packet(this);
    
    // Initialize state
    mTcpSocket = 0;
    mUsePacket = false;

    // Connect signals
    connect(mTcpServer, SIGNAL(newConnection()), this, SLOT(newTcpConnection()));
    connect(mPacket, SIGNAL(dataToSend(QByteArray&)),
            this, SLOT(dataToSend(QByteArray&)));
}

/**
 * Start TCP server on specified port and address.
 * 
 * @param port TCP port to listen on
 * @param addr Network address to bind to (default: Any)
 * @return true if server started successfully, false otherwise
 */
bool TcpServerSimple::startServer(int port, QHostAddress addr)
{
	qDebug() << "in TcpServerSimple::startServer";
	qDebug() << "(adress: " << addr << ", port: " << port;
    
    // Start listening on specified address and port
    if (!mTcpServer->listen(addr,  port)) {
        return false;
    }
    return true;
}

/**
 * Stop the TCP server and clean up the connection.
 * Closes the server, notifies about disconnection, and deletes the socket.
 */
void TcpServerSimple::stopServer()
{
//	qDebug() << "in TcpServerSimple::stoptServer";
    // Close the server
    mTcpServer->close();

    // Clean up the client connection
    if (mTcpSocket) {
        // Notify about disconnection
        emit connectionChanged(false, mTcpSocket->peerAddress().toString());
        
        // Close and delete the socket
        mTcpSocket->close();
        delete mTcpSocket;
        mTcpSocket = 0;
    }
}

/**
 * Send data to the connected client.
 * Also forwards data to ROS 2 channel if available.
 * 
 * @param data Data to send
 * @return true if data was sent successfully, false if no client connected
 */
bool TcpServerSimple::sendData(const QByteArray &data)
{
//    qDebug() << "in TcpServerSimple::sendData: " << data;
    bool res = false;

    // Send data to connected client if available
    if (mTcpSocket) {
        mTcpSocket->write(data);
        sendMessageToRos2(data);
        res = true;
    }

    return res;
}

void TcpServerSimple::sendMessageToRos2(const QByteArray& data)
{
    QLocalSocket socket;
    socket.connectToServer("carclient_ros2_channel");

    if (!socket.waitForConnected(3000)) {
//        qDebug() << "Failed to connect to ROS2 channel";
        return;
    }
    qDebug() << "in TcpServerSimple::sendMessageToRos2: " << data;
    socket.write(data);
    socket.flush();
    socket.disconnectFromServer();
}

QString TcpServerSimple::errorString()
{
    return mTcpServer->errorString();
}

Packet *TcpServerSimple::packet()
{
    return mPacket;
}

void TcpServerSimple::newTcpConnection()
{
    QTcpSocket *socket = mTcpServer->nextPendingConnection();
    if (!socket) {
        return;
    }
    socket->setSocketOption(QAbstractSocket::LowDelayOption, true);
    enableKeepalive(socket);

    // Återanslutning efter en halvöppet tappad 4G-länk: den gamla socketen
    // kan annars ligga kvar i minuter (ingen FIN/RST) och blockera
    // återanslutning (RControlStation fick "TCP Error" och Car_Client verkade
    // hänga sig), och dess buffrade gamla styrkommandon skulle flusha som en
    // skur när länken kom tillbaka -> roboten ryckte till. Vi låter därför en
    // ny anslutning ta över den gamla och slänga den direkt så buffern dör.
    //
    // MEN bara från SAMMA peer-IP (samma operatör som återansluter). En annan
    // peer på WG-nätet får INTE kapa en aktiv styrsession - den avvisas och får
    // vänta tills keepalive (~6 s) städat bort en verkligt död anslutning.
    // (Styrkanalen har ingen egen autentisering; WG är förtroendegränsen, men
    // tyst övertagande mellan olika peers vore en onödig kapningsyta.)
    if (mTcpSocket) {
        if (socket->peerAddress() != mTcpSocket->peerAddress()) {
            qDebug() << "Avvisar ny anslutning från" << socket->peerAddress().toString()
                     << "- aktiv styrsession pågår från"
                     << mTcpSocket->peerAddress().toString();
            socket->abort();
            socket->deleteLater();
            return;
        }
        qDebug() << "Samma peer återansluter, tar över gammal socket från"
                 << mTcpSocket->peerAddress().toString();
        dropSocket(mTcpSocket);
    }

    mTcpSocket = socket;
    connect(mTcpSocket, SIGNAL(readyRead()), this, SLOT(tcpInputDataAvailable()));
    connect(mTcpSocket, SIGNAL(disconnected()),
            this, SLOT(tcpInputDisconnected()));
    connect(mTcpSocket, SIGNAL(error(QAbstractSocket::SocketError)),
            this, SLOT(tcpInputError(QAbstractSocket::SocketError)));
    emit connectionChanged(true, mTcpSocket->peerAddress().toString());
}

void TcpServerSimple::tcpInputDisconnected()
{
    QTcpSocket *socket = qobject_cast<QTcpSocket*>(sender());
    if (!socket) {
        socket = mTcpSocket;
    }
    dropSocket(socket);
}

// Städar bort en socket en gång: kopplar loss signalerna (så vi inte får
// dubbla anrop från abort), avbryter, raderar och meddelar frånkoppling.
// Adressen plockas ut innan abort() eftersom den nollas då.
void TcpServerSimple::dropSocket(QTcpSocket *socket)
{
    if (!socket) {
        return;
    }
    QString addr = socket->peerAddress().toString();
    if (socket == mTcpSocket) {
        mTcpSocket = 0;
    }
    disconnect(socket, 0, this, 0);
    socket->abort();
    socket->deleteLater();
    emit connectionChanged(false, addr);
}

// Snabb keepalive: upptäck en halvöppet död 4G-länk på ~6 s även om klienten
// aldrig återansluter, så den städas bort och roboten bromsas.
void TcpServerSimple::enableKeepalive(QTcpSocket *socket)
{
    if (!socket) {
        return;
    }
    socket->setSocketOption(QAbstractSocket::KeepAliveOption, 1);
#ifdef Q_OS_LINUX
    int fd = socket->socketDescriptor();
    if (fd >= 0) {
        int idle = 3;   // börja proba efter 3 s tystnad
        int intvl = 1;  // proba var 1:e sekund
        int cnt = 3;    // 3 missade prob -> död (~6 s totalt)
        setsockopt(fd, IPPROTO_TCP, TCP_KEEPIDLE, &idle, sizeof(idle));
        setsockopt(fd, IPPROTO_TCP, TCP_KEEPINTVL, &intvl, sizeof(intvl));
        setsockopt(fd, IPPROTO_TCP, TCP_KEEPCNT, &cnt, sizeof(cnt));
    }
#endif
}

void TcpServerSimple::tcpInputDataAvailable()
{
//	qDebug() << "in TcpServerSimple::tcpInputDataAvailable";
    QByteArray data = mTcpSocket->readAll();
    emit dataRx(data);

    if (mUsePacket) {
//        qDebug() << "use Packet";
        mPacket->processData(data);
    } else {
//    	qDebug() << "do not use Packet";
    }
}

void TcpServerSimple::tcpInputError(QAbstractSocket::SocketError socketError)
{
    qDebug() << "TCP-socketfel:" << socketError << "- städar bort anslutningen";
    QTcpSocket *socket = qobject_cast<QTcpSocket*>(sender());
    if (!socket) {
        socket = mTcpSocket;
    }
    dropSocket(socket);
}

void TcpServerSimple::dataToSend(QByteArray &data)
{
    sendData(data);
}

bool TcpServerSimple::usePacket() const
{
    return mUsePacket;
}

void TcpServerSimple::setUsePacket(bool usePacket)
{
    mUsePacket = usePacket;
}
