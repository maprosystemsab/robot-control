/*
	Copyright 2016 - 2018 Benjamin Vedder	benjamin@vedder.se

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

#include "commands.h"
#include "ch.h"
#include "hal.h"
#include "packet.h"
#include "pos.h"
#include "buffer.h"
#include "terminal.h"
#include "bldc_interface.h"
#include "motor_control.h"
#include "servo_simple.h"
#include "utils.h"
#include "sensor_control.h"
#include "state_control.h"
#include "test_sensor_state_control.h"
#include "autopilot.h"
#include "comm_usb.h"
#include "timeout.h"
#include "log.h"
#include "ublox.h"
#include "adconv.h"
#include "styr_position.h"
#include "motor_sim.h"
#include "m8t_base.h"
#include "pos_uwb.h"
#include "fi.h"
#include "comm_can.h"
#include "hydraulic.h"
#include "watchdog.h"

#include <math.h>
#include <string.h>
#include <stdarg.h>
#include <stdio.h>
#include <stdlib.h>

// Commands Module
// This module handles command processing for the RC Controller system.
// It implements the command protocol, processes incoming command packets,
// and executes the appropriate actions based on the command type.
//
// Key Features:
// - Command packet processing and routing
// - Position and navigation control
// - Autopilot route management
// - Sensor data handling
// - Configuration management
// - RTCM3 differential GPS data processing
// - Emergency stop handling
// - CAN bus communication forwarding
// - Logging and debugging support
//
// The module uses a callback-based architecture where the send function is registered
// and commands are processed based on their packet IDs. It supports both direct
// commands and forwarded commands from other devices.

// Defines
#define FWD_TIME		20000 // Forwarding timeout in milliseconds (20 seconds)

// Private variables
static uint8_t m_send_buffer[PACKET_MAX_PL_LEN]; // Send buffer for command packets (max payload length)
static void(*m_send_func)(unsigned char *data, unsigned int len) = 0; // Callback for sending packets
static virtual_timer_t vt; // Virtual timer for timeout handling
static mutex_t m_print_gps; // Mutex for GPS printing synchronization
static bool m_init_done = false; // Flag indicating if module initialization is complete

// Private functions
static void stop_forward(void *p); // Stop forwarding callback (for virtual timer)
static void rtcm_rx(uint8_t *data, int len, int type); // RTCM3 data reception callback
static void rtcm_base_rx(rtcm_ref_sta_pos_t *pos); // RTCM3 base station position callback

// Private variables for sensor data and debugging
static rtcm3_state m_rtcm_state; // RTCM3 state for differential GPS corrections
uint16_t last_sensorvalue; // Last received sensor value (raw ADC)
float debugvalue;    // Debug value 1 (general purpose)
float debugvalue2;   // Debug value 2 (general purpose)
float debugvalue3;   // Debug value 3 (general purpose)
float debugvalue4;   // Debug value 4 (general purpose)
float debugvalue5;   // Debug value 5 (general purpose)
float debugvalue6;   // Debug value 6 (general purpose)
float debugvalue7;   // Debug value 7 (general purpose)
float debugvalue8;   // Debug value 8 (general purpose)
float debugvalue9;   // Debug value 9 (general purpose)
float debugvalue10;  // Debug value 10 (general purpose)
float debugvalue11;  // Debug value 11 (general purpose)
float debugvalue12;  // Debug value 12 (general purpose)
float debugvalue13;  // Debug value 13 (general purpose)
float debugvalue14;  // Debug value 14 (general purpose)
float debugvalue15;  // Debug value 15 (general purpose)
float frontangle=0.0; // Front angle from sensor (degrees)
static bool arduino_connected = false; // Flag indicating if Arduino is connected
bool m_kb_active; // Flag indicating if keyboard control is active

// External variables
extern float io_board_as5047_angle; // IO board AS5047 angle sensor value (degrees)
extern float servo_output; // Current servo output value
extern int iDebug; // External debug flag for conditional debug output
int iCounterCommands=0; // Command counter for statistics/debugging

// Commented out: Emergency stop event (defined elsewhere)
//extern event_source_t emergency_event;

// Commented out: Hydraulic thread (defined elsewhere)
//extern thread_t *hydro_thread;

//extern event_source_t emergency_event;

//extern thread_t *hydro_thread;

// Commented out: Sign function (not currently used)
// Returns the sign of a float value (-1, 0, or 1)
/*
float sign(float input)
	{
	return input/fabs(input);
	}; */

// Initialize the commands module
// This function initializes all module state, sets up RTCM3 callbacks,
// and prepares the module for command processing.
void commands_init(void) {
	// Initialize send function pointer
	m_send_func = 0;
	
	// Initialize synchronization primitives
	chMtxObjectInit(&m_print_gps); // Initialize GPS print mutex
	chVTObjectInit(&vt); // Initialize virtual timer

	// Initialize RTCM3 state and callbacks for differential GPS
	rtcm3_init_state(&m_rtcm_state);
	rtcm3_set_rx_callback(rtcm_rx, &m_rtcm_state); // Set RTCM3 data callback
	rtcm3_set_rx_callback_1005_1006(rtcm_base_rx, &m_rtcm_state); // Set base station position callback

	// Suppress unused variable warning in non-vehicle modes
#if MAIN_MODE != MAIN_MODE_vehicle
	(void)stop_forward;
#endif

	// Initialize debug and sensor variables
	last_sensorvalue=0;
	debugvalue=0.0;
	debugvalue2=0.0;
	debugvalue3=0.0;
	debugvalue4=0.0;
	debugvalue5=0.0;
	debugvalue6=0.0;
	debugvalue7=0.0;
	debugvalue8=0.0;
	debugvalue9=0.0;
	debugvalue10=0.0;
	debugvalue11=0.0;
	debugvalue12=0.0;
	
	// Set initialization flag
	m_init_done = true;
	m_kb_active=false;
}

/**
 * Provide a function to use the next time there are packets to be sent.
 * This function registers the callback that will be used to transmit command packets.
 *
 * @param func
 * A pointer to the packet sending function with signature: void func(unsigned char *data, unsigned int len)
 */
void commands_set_send_func(void(*func)(unsigned char *data, unsigned int len)) {
	m_send_func = func; // Register the send callback
}

/**
 * Send a packet using the set send function.
 * This function transmits a command packet using the callback registered with commands_set_send_func().
 *
 * @param data
 * The packet data to send (byte array).
 *
 * @param len
 * The length of the data in bytes.
 */
void commands_send_packet(unsigned char *data, unsigned int len) {
	// Only send if send function is registered
	if (m_send_func) {
		m_send_func(data, len); // Invoke the registered send callback
	}
}

/**
 * Process a received buffer with commands and data.
 * This is the main command processing function that parses incoming command packets
 * and executes the appropriate actions based on the command ID.
 *
 * Packet format:
 * - Byte 0: Device ID (target device)
 * - Byte 1: Command ID (packet_id)
 * - Remaining bytes: Command-specific data
 *
 * @param data
 * The buffer to process (byte array containing device ID, command ID, and data).
 *
 * @param len
 * The length of the buffer in bytes.
 *
 * @param func
 * A pointer to the packet sending function for sending responses.
 */
void commands_process_packet(unsigned char *data, unsigned int len,
		void (*func)(unsigned char *data, unsigned int len)) {
	// Return immediately if buffer is empty
	if (!len) {
		return;
	}
	
	// Check for RTCM3 differential GPS data (special handling)
	// RTCM3 packets start with a specific preamble byte
	if (data[0] == RTCM3PREAMB) {
		// Process each byte as RTCM3 data
		for (unsigned int i = 0;i < len;i++) {
			rtcm3_input_data(data[i], &m_rtcm_state); // Feed data to RTCM3 parser
		}
		return; // RTCM3 data is handled separately
	}

	// Parse the command packet
	CMD_PACKET packet_id; // Command ID from the packet
	uint8_t id = 0; // Device ID

	id = data[0]; // Extract device ID from first byte
	data++; // Move to command ID
	len--; // Decrement length

	packet_id = data[0]; // Extract command ID
	data++; // Move to data portion
	len--; // Decrement length

	// Check if this packet is for us (or broadcast)
	// Accept packets addressed to:
	// - Our main_id
	// - ID_ALL (broadcast to all devices)
	// - ID_VEHICLE_CLIENT (vehicle client)
	if (id == main_id || id == 0 || id == ID_ALL || id == ID_VEHICLE_CLIENT) {
		int id_ret = main_id; // Default response ID is our main_id

		// If packet was addressed to ID_VEHICLE_CLIENT, respond with that ID
		if (id == ID_VEHICLE_CLIENT) {
			id_ret = ID_VEHICLE_CLIENT;
		}
		
		// Process the command based on packet_id
		switch (packet_id) {
		// ==================== General commands ==================== //
		
		// Terminal command (CMD_TERMINAL_CMD = 1)
		// Executes a terminal command string. Used for sending text commands to the system.
		// Data format: Null-terminated string
		case CMD_TERMINAL_CMD: {
			timeout_reset(); // Reset the system timeout (prevents watchdog reset)
			commands_set_send_func(func); // Register send function for any responses

			data[len] = '\0'; // Null-terminate the command string
			terminal_process_string((char*)data); // Process the command string
		} break;

		// ==================== Vehicle commands ==================== //

		// Heartbeat command (CMD_HEARTBEAT = 89)
		// Currently commented out. Would signal heartbeat to watchdog.
/*		case CMD_HEARTBEAT:
            chEvtBroadcast(&heartbeat_event); // Signal heartbeat to watchdog
	        break;*/
		
		// Arduino status (CMD_ARDUINO_STATUS)
		// Reports the connection status of an Arduino device.
		// Data format: [status (1 byte, 0=disconnected, 1=connected)]
		case CMD_ARDUINO_STATUS   :
			if (data[0]) // Check if status byte is non-zero
			{
				arduino_connected=true; // Set Arduino connected flag
			} else
			{
				arduino_connected=false; // Clear Arduino connected flag
			}
			break;

		// Get angle from sensor (CMD_GETANGLE = 100)
		// Receives angle sensor data, validates it, and calculates the front angle.
		// Data format: [start_byte (0xAA), voltage_high (1 byte), voltage_low (1 byte), checksum (1 byte)]
		case CMD_GETANGLE: {
		    // Check packet length (6 bytes: car_id, cmd, start, 2 data bytes, checksum)
		    if (len < 4) {
		    	commands_printf("Error: Packet too short\n");
		        break;
		    }

		    // Check start byte (third byte in the packet)
		    if (data[0] != 0xAA) {
		    	commands_printf("Error: Invalid start byte\n");
		        break;
		    }

		    // Calculate checksum (XOR of start byte + 2 data bytes)
		    char calculatedChecksum = 0;
		    calculatedChecksum ^= data[0]; // Start byte
		    calculatedChecksum ^= data[1]; // High byte
		    calculatedChecksum ^= data[2]; // Low byte

		    // Validate checksum
		    if (calculatedChecksum != data[3]) {
		    	commands_printf("Error: Checksum mismatch\n");
		        break;
		    }

		    // Extract the 2-byte scaled voltage (big-endian)
		    uint16_t scaledVoltage = (data[1] << 8) | data[2];
		    float voltage = scaledVoltage / 1000.0f;


		    // Calculate angle
			last_sensorvalue = voltage;

		    frontangle = (voltage - main_config.vehicle.sensorcentre) * (main_config.vehicle.degreeinterval / main_config.vehicle.sensorinterval);
		    comm_can_io_board_as5047_setangle(frontangle);
		    if (iDebug==31)
		    {
		    	commands_printf("Voltage: %u : %.3f, Angle: %.3f, centre: : %.3f, degreeinterval:%.3f, sensorinterval:%.3f\n",
					scaledVoltage, voltage,  frontangle, main_config.vehicle.sensorcentre, main_config.vehicle.degreeinterval, main_config.vehicle.sensorinterval);
		    };
		    // Debug output
		    break;
		}

		case CMD_SET_POS:
		case CMD_SET_POS_ACK: {
			timeout_reset();

			float x, y, angle;
			int32_t ind = 0;
			x = buffer_get_float32(data, 1e4, &ind);
			y = buffer_get_float32(data, 1e4, &ind);
			angle = buffer_get_float32(data, 1e6, &ind);
			pos_set_xya(x, y, angle);
			pos_uwb_set_xya(x, y, angle);

			if (packet_id == CMD_SET_POS_ACK) {
				commands_set_send_func(func);
				// Send ack
				int32_t send_index = 0;
				m_send_buffer[send_index++] = id_ret;
				m_send_buffer[send_index++] = packet_id;
				commands_send_packet(m_send_buffer, send_index);
			}
		} break;

		case CMD_SET_ENU_REF: {
			timeout_reset();
			commands_set_send_func(func);

			int32_t ind = 0;
			double lat, lon, height;
			lat = buffer_get_double64(data, D(1e16), &ind);
			lon = buffer_get_double64(data, D(1e16), &ind);
			height = buffer_get_float32(data, 1e3, &ind);
			pos_set_enu_ref(lat, lon, height);

			// Send ack
			int32_t send_index = 0;
			m_send_buffer[send_index++] = id_ret;
			m_send_buffer[send_index++] = packet_id;
			commands_send_packet(m_send_buffer, send_index);
		} break;

		case CMD_GET_ENU_REF: {
			timeout_reset();
			commands_set_send_func(func);

			double llh[3];
			pos_get_enu_ref(llh);

			int32_t send_index = 0;
			m_send_buffer[send_index++] = id_ret;
			m_send_buffer[send_index++] = CMD_GET_ENU_REF;
			buffer_append_double64(m_send_buffer, llh[0], D(1e16), &send_index);
			buffer_append_double64(m_send_buffer, llh[1], D(1e16), &send_index);
			buffer_append_float32(m_send_buffer, llh[2], 1e3, &send_index);
			commands_send_packet(m_send_buffer, send_index);
		} break;

		case CMD_AP_ADD_POINTS: {
			timeout_reset();
			commands_set_send_func(func);

			int32_t ind = 0;
			bool first = true;

			while (ind < (int32_t)len) {
				ROUTE_POINT p;
				p.px = buffer_get_float32(data, 1e4, &ind);
				p.py = buffer_get_float32(data, 1e4, &ind);
				p.pz = buffer_get_float32(data, 1e4, &ind);
				p.speed = buffer_get_float32(data, 1e6, &ind);
				p.time = buffer_get_int32(data, &ind);
				p.attributes = buffer_get_uint32(data, &ind);
				bool res = autopilot_add_point(&p, first);
				first = false;

				if (!res) {
					break;
				}
			}

			// Send ack
			int32_t send_index = 0;
			m_send_buffer[send_index++] = id_ret;
			m_send_buffer[send_index++] = packet_id;
			commands_send_packet(m_send_buffer, send_index);
		} break;

		case CMD_AP_REMOVE_LAST_POINT: {
			timeout_reset();
			commands_set_send_func(func);

			autopilot_remove_last_point();

			// Send ack
			int32_t send_index = 0;
			m_send_buffer[send_index++] = id_ret;
			m_send_buffer[send_index++] = packet_id;
			commands_send_packet(m_send_buffer, send_index);
		} break;

		case CMD_AP_CLEAR_POINTS: {
			timeout_reset();
			commands_set_send_func(func);

			autopilot_clear_route();

			// Send ack
			int32_t send_index = 0;
			m_send_buffer[send_index++] = id_ret;
			m_send_buffer[send_index++] = packet_id;
			commands_send_packet(m_send_buffer, send_index);
		} break;

		case CMD_AP_GET_ROUTE_PART: {
			int32_t ind = 0;
			int first = buffer_get_int32(data, &ind);
			int num = data[ind++];

			if (num > 20) {
				break;
			}

			int32_t send_index = 0;
			m_send_buffer[send_index++] = id_ret;
			m_send_buffer[send_index++] = CMD_AP_GET_ROUTE_PART;

			int route_len = autopilot_get_route_len();
			buffer_append_int32(m_send_buffer, route_len, &send_index);

			for (int i = first;i < (first + num);i++) {
				ROUTE_POINT rp = autopilot_get_route_point(i);
				buffer_append_float32_auto(m_send_buffer, rp.px, &send_index);
				buffer_append_float32_auto(m_send_buffer, rp.py, &send_index);
				buffer_append_float32_auto(m_send_buffer, rp.pz, &send_index);
				buffer_append_float32_auto(m_send_buffer, rp.speed, &send_index);
				buffer_append_int32(m_send_buffer, rp.time, &send_index);
				buffer_append_uint32(m_send_buffer, rp.attributes, &send_index);
			}

			commands_send_packet(m_send_buffer, send_index);
		} break;

		case CMD_AP_SET_ACTIVE: {
			timeout_reset();
			commands_set_send_func(func);

			autopilot_set_active(data[0]);
			if (data[1])
				autopilot_reset_state();

			// Send ack
			int32_t send_index = 0;
			m_send_buffer[send_index++] = id_ret;
			m_send_buffer[send_index++] = packet_id;
			commands_send_packet(m_send_buffer, send_index);
		} break;
		case CMD_KB_SET_ACTIVE: {
			m_kb_active=data[1];

			// Send ack
			int32_t send_index = 0;
			m_send_buffer[send_index++] = id_ret;
			m_send_buffer[send_index++] = packet_id;
			commands_send_packet(m_send_buffer, send_index);
		} break;


		case CMD_AP_REPLACE_ROUTE: {
			timeout_reset();
			commands_set_send_func(func);

			int32_t ind = 0;
			int first = true;

			while (ind < (int32_t)len) {
				ROUTE_POINT p;
				p.px = buffer_get_float32(data, 1e4, &ind);
				p.py = buffer_get_float32(data, 1e4, &ind);
				p.pz = buffer_get_float32(data, 1e4, &ind);
				p.speed = buffer_get_float32(data, 1e6, &ind);
				p.time = buffer_get_int32(data, &ind);
				p.attributes = buffer_get_uint32(data, &ind);

				if (first) {
					first = !autopilot_replace_route(&p);
				} else {
					autopilot_add_point(&p, false);
				}
			}

			// Send ack
			int32_t send_index = 0;
			m_send_buffer[send_index++] = id_ret;
			m_send_buffer[send_index++] = packet_id;
			commands_send_packet(m_send_buffer, send_index);
		} break;

		case CMD_AP_SYNC_POINT: {
			timeout_reset();
			commands_set_send_func(func);

			int32_t ind = 0;
			int32_t point = buffer_get_int32(data, &ind);
			int32_t time = buffer_get_int32(data, &ind);
			int32_t min_diff = buffer_get_int32(data, &ind);

			autopilot_sync_point(point, time, min_diff);

			// Send ack
			int32_t send_index = 0;
			m_send_buffer[send_index++] = id_ret;
			m_send_buffer[send_index++] = packet_id;
			commands_send_packet(m_send_buffer, send_index);
		} break;

		case CMD_SEND_RTCM_USB: {
#if UBLOX_EN
			ublox_send(data, len);
#endif
			for (unsigned int i = 0;i < len;i++) {
				rtcm3_input_data(data[i], &m_rtcm_state);
			}
		} break;

		case CMD_SEND_NMEA_RADIO: {
#if !UBLOX_EN
			// Only enable this command if the board is configured without the ublox
			char *curLine = (char*)data;
			while(curLine) {
				char *nextLine = strchr(curLine, '\n');
				if (nextLine) {
					*nextLine = '\0';
				}

				bool found = pos_input_nmea(curLine);

				// Only send the lines that pos decoded
				if (found && main_config.gps_send_nmea) {
					int32_t send_index = 0;
					m_send_buffer[send_index++] = id_ret;
					m_send_buffer[send_index++] = packet_id;
					int len_line = strlen(curLine);
					memcpy(m_send_buffer + send_index, curLine, len_line);
					send_index += len_line;

					commands_send_packet(m_send_buffer, send_index);
				}

				if (nextLine) {
					*nextLine = '\n';
				}

				curLine = nextLine ? (nextLine + 1) : NULL;
			}
#endif
		} break;

		case CMD_SET_YAW_OFFSET:
		case CMD_SET_YAW_OFFSET_ACK: {
			timeout_reset();

			float angle;
			int32_t ind = 0;
			angle = buffer_get_float32(data, 1e6, &ind);
			pos_set_yaw_offset(angle);

			if (packet_id == CMD_SET_YAW_OFFSET_ACK) {
				commands_set_send_func(func);
				// Send ack
				int32_t send_index = 0;
				m_send_buffer[send_index++] = id_ret;
				m_send_buffer[send_index++] = packet_id;
				commands_send_packet(m_send_buffer, send_index);
			}
		} break;

		case CMD_SET_MS_TODAY: {
			timeout_reset();

			int32_t time;
			int32_t ind = 0;
			time = buffer_get_int32(data, &ind);
			pos_set_ms_today(time);
		} break;

		case CMD_SET_SYSTEM_TIME: {
			commands_set_send_func(func);

			int32_t send_index = 0;
			m_send_buffer[send_index++] = id_ret;
			m_send_buffer[send_index++] = packet_id;
			memcpy(m_send_buffer + send_index, data, len);
			send_index += len;
			comm_usb_send_packet(m_send_buffer, send_index);

			// Send ack
			send_index = 0;
			m_send_buffer[send_index++] = id_ret;
			m_send_buffer[send_index++] = CMD_SET_SYSTEM_TIME_ACK;
			commands_send_packet(m_send_buffer, send_index);
		} break;

		case CMD_REBOOT_SYSTEM: {
			commands_set_send_func(func);

			int32_t send_index = 0;
			m_send_buffer[send_index++] = id_ret;
			m_send_buffer[send_index++] = packet_id;
			memcpy(m_send_buffer + send_index, data, len);
			send_index += len;
			comm_usb_send_packet(m_send_buffer, send_index);

			// Send ack
			send_index = 0;
			m_send_buffer[send_index++] = id_ret;
			m_send_buffer[send_index++] = CMD_REBOOT_SYSTEM_ACK;
			commands_send_packet(m_send_buffer, send_index);
			commands_sleep();			// Stop execution on the card
		} break;

		case CMD_SET_MAIN_CONFIG: {
			timeout_reset();
			commands_set_send_func(func);

			// Stoppa maskinen innan inställningarna ändras och sparas: sparningen i
			// flash står still en stund, och körning med halvt ändrade värden vill
			// ingen ha. (Gunnars förslag 2026-09-28.)
			autopilot_set_active(false);
			motor_stop();

			int32_t ind = 0;
			main_config.mag_use = data[ind++];
			main_config.mag_comp = data[ind++];
			main_config.yaw_mag_gain = buffer_get_float32_auto(data, &ind);

			main_config.mag_cal_cx = buffer_get_float32_auto(data, &ind);
			main_config.mag_cal_cy = buffer_get_float32_auto(data, &ind);
			main_config.mag_cal_cz = buffer_get_float32_auto(data, &ind);
			main_config.mag_cal_xx = buffer_get_float32_auto(data, &ind);
			main_config.mag_cal_xy = buffer_get_float32_auto(data, &ind);
			main_config.mag_cal_xz = buffer_get_float32_auto(data, &ind);
			main_config.mag_cal_yx = buffer_get_float32_auto(data, &ind);
			main_config.mag_cal_yy = buffer_get_float32_auto(data, &ind);
			main_config.mag_cal_yz = buffer_get_float32_auto(data, &ind);
			main_config.mag_cal_zx = buffer_get_float32_auto(data, &ind);
			main_config.mag_cal_zy = buffer_get_float32_auto(data, &ind);
			main_config.mag_cal_zz = buffer_get_float32_auto(data, &ind);

			main_config.gps_ant_x = buffer_get_float32_auto(data, &ind);
			main_config.gps_ant_y = buffer_get_float32_auto(data, &ind);
			main_config.gps_comp = data[ind++];
			main_config.gps_req_rtk = data[ind++];
			main_config.gps_use_rtcm_base_as_enu_ref = data[ind++];
			main_config.gps_corr_gain_stat = buffer_get_float32_auto(data, &ind);
			main_config.gps_corr_gain_dyn = buffer_get_float32_auto(data, &ind);
			main_config.gps_corr_gain_yaw = buffer_get_float32_auto(data, &ind);
			main_config.gps_send_nmea = data[ind++];
			main_config.gps_use_ubx_info = data[ind++];
			main_config.gps_ubx_max_acc = buffer_get_float32_auto(data, &ind);

			main_config.uwb_max_corr = buffer_get_float32_auto(data, &ind);

			main_config.ap_repeat_routes = data[ind++];
			main_config.ap_base_rad = buffer_get_float32_auto(data, &ind);
			main_config.ap_rad_time_ahead = buffer_get_float32_auto(data, &ind);
			main_config.ap_mode_time = data[ind++];
			main_config.ap_max_speed = buffer_get_float32_auto(data, &ind);
			main_config.ap_time_add_repeat_ms = buffer_get_int32(data, &ind);

			main_config.log_rate_hz = buffer_get_int16(data, &ind);
			main_config.log_en = data[ind++];
			strcpy(main_config.log_name, (const char*)(data + ind));
			ind += strlen(main_config.log_name) + 1;
			main_config.log_mode_ext = data[ind++];
			main_config.log_uart_baud = buffer_get_uint32(data, &ind);

			log_set_rate(main_config.log_rate_hz);
			log_set_enabled(main_config.log_en);
			log_set_name(main_config.log_name);
			log_set_ext(main_config.log_mode_ext, main_config.log_uart_baud);

			// vehicle settings
			main_config.vehicle.yaw_use_odometry = data[ind++];
			main_config.vehicle.yaw_imu_gain = buffer_get_float32_auto(data, &ind);
			main_config.vehicle.disable_motor = data[ind++];
			main_config.vehicle.simulate_motor = data[ind++];
			main_config.vehicle.clamp_imu_yaw_stationary = data[ind++];
			main_config.vehicle.use_uwb_pos = data[ind++];

			main_config.vehicle.gear_ratio = buffer_get_float32_auto(data, &ind);
			main_config.vehicle.wheel_diam = buffer_get_float32_auto(data, &ind);
			main_config.vehicle.motor_poles = buffer_get_float32_auto(data, &ind);
			main_config.vehicle.steering_max_angle_rad = buffer_get_float32_auto(data, &ind);
			main_config.vehicle.steering_center = buffer_get_float32_auto(data, &ind);
			main_config.vehicle.steering_range = buffer_get_float32_auto(data, &ind);
			main_config.vehicle.steering_ramp_time = buffer_get_float32_auto(data, &ind);
			main_config.vehicle.axis_distance = buffer_get_float32_auto(data, &ind);
			main_config.vehicle.vesc_p_gain = buffer_get_float32_auto(data, &ind);
			main_config.vehicle.vesc_i_gain = buffer_get_float32_auto(data, &ind);
			main_config.vehicle.vesc_d_gain = buffer_get_float32_auto(data, &ind);

			main_config.vehicle.sensorcentre = buffer_get_float32_auto(data, &ind);
			main_config.vehicle.sensorinterval = buffer_get_float32_auto(data, &ind);
			main_config.vehicle.degreeinterval = buffer_get_float32_auto(data, &ind);
			float deadband= buffer_get_float32_auto(data, &ind);
			main_config.vehicle.deadband =deadband;

			// Fälten nedan finns bara i nyare RControlStation, och RControlStation
			// skickar i dag bara t.o.m. aktuatorerna. Läs ett fält bara om det finns i
			// paketet: förut läste kortet vidare in i gamla bytes i mottagningsbufferten
			// (GPS-text), sparade dem som sensorer/reglerloopar, och nästa Write
			// startade reglerloopar med skräpindex -> HardFault, kortet hängde.
			if (ind + 4 <= (int32_t)len) {
				main_config.vehicle.heartbeat_maxtime = buffer_get_float32_auto(data, &ind);
			}
			if (ind + 2 + 4 * 8 <= (int32_t)len) {
				main_config.vehicle.actuators = buffer_get_uint16(data, &ind);
				for (int i=0;i<4;i++)
				{
					main_config.vehicle.actuator[i].type=buffer_get_uint16(data, &ind);;
					main_config.vehicle.actuator[i].motorid=buffer_get_uint16(data, &ind);;
					main_config.vehicle.actuator[i].activity=buffer_get_uint16(data, &ind);;
					main_config.vehicle.actuator[i].mode=buffer_get_uint16(data, &ind);;
				};
			}

			// Sensor configurations
			if (ind + 2 + 4 * 8 <= (int32_t)len) {
				main_config.vehicle.sensors = buffer_get_uint16(data, &ind);
				for (int i=0;i<4;i++)
				{
					main_config.vehicle.sensor[i].type=buffer_get_uint16(data, &ind);;
					main_config.vehicle.sensor[i].sensorid=buffer_get_uint16(data, &ind);;
					main_config.vehicle.sensor[i].activity=buffer_get_uint16(data, &ind);;
					main_config.vehicle.sensor[i].reserved=buffer_get_uint16(data, &ind);;
				};
			} else {
				main_config.vehicle.sensors = 0;
			}

			// State control configurations (3 uint16 + 6 float32 + 1 byte per loop)
			if (ind + 2 + 4 * 31 <= (int32_t)len) {
				main_config.vehicle.state_controls = buffer_get_uint16(data, &ind);
				for (int i=0;i<4;i++)
				{
					main_config.vehicle.control[i].actuator_activity=buffer_get_uint16(data, &ind);;
					main_config.vehicle.control[i].sensor_activity=buffer_get_uint16(data, &ind);;
					main_config.vehicle.control[i].control_type=buffer_get_uint16(data, &ind);;
					main_config.vehicle.control[i].target_value=buffer_get_float32_auto(data, &ind);;
					main_config.vehicle.control[i].kp=buffer_get_float32_auto(data, &ind);;
					main_config.vehicle.control[i].ki=buffer_get_float32_auto(data, &ind);;
					main_config.vehicle.control[i].kd=buffer_get_float32_auto(data, &ind);;
					main_config.vehicle.control[i].min_output=buffer_get_float32_auto(data, &ind);;
					main_config.vehicle.control[i].max_output=buffer_get_float32_auto(data, &ind);;
					main_config.vehicle.control[i].enabled=data[ind++];;
				};
			} else {
				main_config.vehicle.state_controls = 0;
			}
			conf_general_sanitize_main_config(&main_config);

			motor_sim_set_running(main_config.vehicle.simulate_motor);
			conf_general_store_main_config(&main_config);
			// Doing this while driving will get wrong as there is so much accelerometer noise then.
			//pos_reset_attitude();

			// Först nu, med de nya värdena (förut kördes detta före tolkningen och
			// läste då de gamla, ev. trasiga, värdena från EEPROM).
			sensor_control_init();
			state_control_init();

			// Nytt heartbeat gäller direkt (förut först efter omstart av kortet).
			timeout_configure(timeout_heartbeat_ms(main_config.vehicle.heartbeat_maxtime),
					timeout_get_brake_current());

			// Send ack
			int32_t send_index = 0;
			m_send_buffer[send_index++] = id_ret;
			m_send_buffer[send_index++] = packet_id;
			commands_send_packet(m_send_buffer, send_index);
		} break;

		case CMD_GET_MAIN_CONFIG:
		case CMD_GET_MAIN_CONFIG_DEFAULT: {
			timeout_reset();
			commands_set_send_func(func);

			MAIN_CONFIG main_cfg_tmp;

			if (packet_id == CMD_GET_MAIN_CONFIG) {
				main_cfg_tmp = main_config;
			} else {
				conf_general_get_default_main_config(&main_cfg_tmp);
			}

			int32_t send_index = 0;
			m_send_buffer[send_index++] = id_ret;
			m_send_buffer[send_index++] = packet_id;

			m_send_buffer[send_index++] = main_cfg_tmp.mag_use;
			m_send_buffer[send_index++] = main_cfg_tmp.mag_comp;
			buffer_append_float32_auto(m_send_buffer, main_cfg_tmp.yaw_mag_gain, &send_index);

			buffer_append_float32_auto(m_send_buffer, main_cfg_tmp.mag_cal_cx, &send_index);
			buffer_append_float32_auto(m_send_buffer, main_cfg_tmp.mag_cal_cy, &send_index);
			buffer_append_float32_auto(m_send_buffer, main_cfg_tmp.mag_cal_cz, &send_index);
			buffer_append_float32_auto(m_send_buffer, main_cfg_tmp.mag_cal_xx, &send_index);
			buffer_append_float32_auto(m_send_buffer, main_cfg_tmp.mag_cal_xy, &send_index);
			buffer_append_float32_auto(m_send_buffer, main_cfg_tmp.mag_cal_xz, &send_index);
			buffer_append_float32_auto(m_send_buffer, main_cfg_tmp.mag_cal_yx, &send_index);
			buffer_append_float32_auto(m_send_buffer, main_cfg_tmp.mag_cal_yy, &send_index);
			buffer_append_float32_auto(m_send_buffer, main_cfg_tmp.mag_cal_yz, &send_index);
			buffer_append_float32_auto(m_send_buffer, main_cfg_tmp.mag_cal_zx, &send_index);
			buffer_append_float32_auto(m_send_buffer, main_cfg_tmp.mag_cal_zy, &send_index);
			buffer_append_float32_auto(m_send_buffer, main_cfg_tmp.mag_cal_zz, &send_index);

			buffer_append_float32_auto(m_send_buffer, main_cfg_tmp.gps_ant_x, &send_index);
			buffer_append_float32_auto(m_send_buffer, main_cfg_tmp.gps_ant_y, &send_index);
			m_send_buffer[send_index++] = main_cfg_tmp.gps_comp;
			m_send_buffer[send_index++] = main_cfg_tmp.gps_req_rtk;
			m_send_buffer[send_index++] = main_cfg_tmp.gps_use_rtcm_base_as_enu_ref;
			buffer_append_float32_auto(m_send_buffer, main_cfg_tmp.gps_corr_gain_stat, &send_index);
			buffer_append_float32_auto(m_send_buffer, main_cfg_tmp.gps_corr_gain_dyn, &send_index);
			buffer_append_float32_auto(m_send_buffer, main_cfg_tmp.gps_corr_gain_yaw, &send_index);
			m_send_buffer[send_index++] = main_cfg_tmp.gps_send_nmea;
			m_send_buffer[send_index++] = main_cfg_tmp.gps_use_ubx_info;
			buffer_append_float32_auto(m_send_buffer, main_cfg_tmp.gps_ubx_max_acc, &send_index);

			buffer_append_float32_auto(m_send_buffer, main_cfg_tmp.uwb_max_corr, &send_index);

			m_send_buffer[send_index++] = main_cfg_tmp.ap_repeat_routes;
			buffer_append_float32_auto(m_send_buffer, main_cfg_tmp.ap_base_rad, &send_index);
			buffer_append_float32_auto(m_send_buffer, main_cfg_tmp.ap_rad_time_ahead, &send_index);
			m_send_buffer[send_index++] = main_cfg_tmp.ap_mode_time;
			buffer_append_float32_auto(m_send_buffer, main_cfg_tmp.ap_max_speed, &send_index);
			buffer_append_int32(m_send_buffer, main_cfg_tmp.ap_time_add_repeat_ms, &send_index);

			buffer_append_int16(m_send_buffer, main_cfg_tmp.log_rate_hz, &send_index);
			m_send_buffer[send_index++] = main_cfg_tmp.log_en;
			strcpy((char*)(m_send_buffer + send_index), main_cfg_tmp.log_name);
			send_index += strlen(main_config.log_name) + 1;
			m_send_buffer[send_index++] = main_cfg_tmp.log_mode_ext;
			buffer_append_uint32(m_send_buffer, main_cfg_tmp.log_uart_baud, &send_index);

			// vehicle settings
			m_send_buffer[send_index++] = main_cfg_tmp.vehicle.yaw_use_odometry;
			buffer_append_float32_auto(m_send_buffer, main_cfg_tmp.vehicle.yaw_imu_gain, &send_index);
			m_send_buffer[send_index++] = main_cfg_tmp.vehicle.disable_motor;
			m_send_buffer[send_index++] = main_cfg_tmp.vehicle.simulate_motor;
			m_send_buffer[send_index++] = main_cfg_tmp.vehicle.clamp_imu_yaw_stationary;
			m_send_buffer[send_index++] = main_cfg_tmp.vehicle.use_uwb_pos;

			buffer_append_float32_auto(m_send_buffer, main_cfg_tmp.vehicle.gear_ratio, &send_index);
			buffer_append_float32_auto(m_send_buffer, main_cfg_tmp.vehicle.wheel_diam, &send_index);
			buffer_append_float32_auto(m_send_buffer, main_cfg_tmp.vehicle.motor_poles, &send_index);
			buffer_append_float32_auto(m_send_buffer, main_cfg_tmp.vehicle.steering_max_angle_rad, &send_index);
			buffer_append_float32_auto(m_send_buffer, main_cfg_tmp.vehicle.steering_center, &send_index);
			buffer_append_float32_auto(m_send_buffer, main_cfg_tmp.vehicle.steering_range, &send_index);
			buffer_append_float32_auto(m_send_buffer, main_cfg_tmp.vehicle.steering_ramp_time, &send_index);
			buffer_append_float32_auto(m_send_buffer, main_cfg_tmp.vehicle.axis_distance, &send_index);
			buffer_append_float32_auto(m_send_buffer, main_cfg_tmp.vehicle.vesc_p_gain, &send_index);
			buffer_append_float32_auto(m_send_buffer, main_cfg_tmp.vehicle.vesc_i_gain, &send_index);
			buffer_append_float32_auto(m_send_buffer, main_cfg_tmp.vehicle.vesc_d_gain, &send_index);

		    buffer_append_float32_auto(m_send_buffer, main_cfg_tmp.vehicle.sensorcentre, &send_index);
		    buffer_append_float32_auto(m_send_buffer, main_cfg_tmp.vehicle.sensorinterval, &send_index);
		    buffer_append_float32_auto(m_send_buffer, main_cfg_tmp.vehicle.degreeinterval, &send_index);
		    buffer_append_float32_auto(m_send_buffer, main_cfg_tmp.vehicle.deadband, &send_index);
		    buffer_append_float32_auto(m_send_buffer, main_cfg_tmp.vehicle.heartbeat_maxtime, &send_index);
		    commands_printf("antal skrivna aktuatorer: %u",main_cfg_tmp.vehicle.actuators);
		    buffer_append_uint16(m_send_buffer, main_cfg_tmp.vehicle.actuators, &send_index);

			for (int i=0;i<4;i++)
			{
			    buffer_append_uint16(m_send_buffer, main_cfg_tmp.vehicle.actuator[i].type, &send_index);
			    buffer_append_uint16(m_send_buffer, main_cfg_tmp.vehicle.actuator[i].motorid, &send_index);
			    buffer_append_uint16(m_send_buffer, main_cfg_tmp.vehicle.actuator[i].activity, &send_index);
			    buffer_append_uint16(m_send_buffer, main_cfg_tmp.vehicle.actuator[i].mode, &send_index);
			};
		    commands_printf("read deadband: %f",main_cfg_tmp.vehicle.deadband);
			
			// Sensor configurations
			commands_printf("antal skrivna sensorer: %u",main_cfg_tmp.vehicle.sensors);
			buffer_append_uint16(m_send_buffer, main_cfg_tmp.vehicle.sensors, &send_index);
			for (int i=0;i<4;i++)
			{
			    buffer_append_uint16(m_send_buffer, main_cfg_tmp.vehicle.sensor[i].type, &send_index);
			    buffer_append_uint16(m_send_buffer, main_cfg_tmp.vehicle.sensor[i].sensorid, &send_index);
			    buffer_append_uint16(m_send_buffer, main_cfg_tmp.vehicle.sensor[i].activity, &send_index);
			    buffer_append_uint16(m_send_buffer, main_cfg_tmp.vehicle.sensor[i].reserved, &send_index);
			}
			
			// State control configurations
			commands_printf("antal skrivna state controls: %u",main_cfg_tmp.vehicle.state_controls);
			buffer_append_uint16(m_send_buffer, main_cfg_tmp.vehicle.state_controls, &send_index);
			for (int i=0;i<4;i++)
			{
			    buffer_append_uint16(m_send_buffer, main_cfg_tmp.vehicle.control[i].actuator_activity, &send_index);
			    buffer_append_uint16(m_send_buffer, main_cfg_tmp.vehicle.control[i].sensor_activity, &send_index);
			    buffer_append_uint16(m_send_buffer, main_cfg_tmp.vehicle.control[i].control_type, &send_index);
			    buffer_append_float32_auto(m_send_buffer, main_cfg_tmp.vehicle.control[i].target_value, &send_index);
			    buffer_append_float32_auto(m_send_buffer, main_cfg_tmp.vehicle.control[i].kp, &send_index);
			    buffer_append_float32_auto(m_send_buffer, main_cfg_tmp.vehicle.control[i].ki, &send_index);
			    buffer_append_float32_auto(m_send_buffer, main_cfg_tmp.vehicle.control[i].kd, &send_index);
			    buffer_append_float32_auto(m_send_buffer, main_cfg_tmp.vehicle.control[i].min_output, &send_index);
			    buffer_append_float32_auto(m_send_buffer, main_cfg_tmp.vehicle.control[i].max_output, &send_index);
			    m_send_buffer[send_index++] = main_cfg_tmp.vehicle.control[i].enabled;
			}
			commands_send_packet(m_send_buffer, send_index);
		} break;

		case CMD_ADD_UWB_ANCHOR: {
			int32_t ind = 0;
			UWB_ANCHOR a;

			a.id = buffer_get_int16(data, &ind);
			a.px = buffer_get_float32_auto(data, &ind);
			a.py = buffer_get_float32_auto(data, &ind);
			a.height = buffer_get_float32_auto(data, &ind);
			a.dist_last = 0.0;
			pos_uwb_add_anchor(a);

			// Send ack
			int32_t send_index = 0;
			m_send_buffer[send_index++] = id_ret;
			m_send_buffer[send_index++] = packet_id;
			commands_send_packet(m_send_buffer, send_index);
		} break;

		case CMD_CLEAR_UWB_ANCHORS: {
			pos_uwb_clear_anchors();

			// Send ack
			int32_t send_index = 0;
			m_send_buffer[send_index++] = id_ret;
			m_send_buffer[send_index++] = packet_id;
			commands_send_packet(m_send_buffer, send_index);
		} break;

		case CMD_IO_BOARD_SET_PWM_DUTY: {
			int32_t ind = 0;
			uint8_t board = data[ind++];
			float value = buffer_get_float32_auto(data, &ind);
			comm_can_io_board_set_pwm_duty(board, value);

			// Send ack
			int32_t send_index = 0;
			m_send_buffer[send_index++] = id_ret;
			m_send_buffer[send_index++] = packet_id;
			commands_send_packet(m_send_buffer, send_index);
		} break;

		case CMD_IO_BOARD_SET_VALVE: {
			comm_can_io_board_set_valve(data[0], data[1], data[2]);

			// Send ack
			int32_t send_index = 0;
			m_send_buffer[send_index++] = id_ret;
			m_send_buffer[send_index++] = packet_id;
			commands_send_packet(m_send_buffer, send_index);
		} break;

		case CMD_HYDRAULIC_MOVE:
			// L1,L2 R1,R2
			hydraulic_move(data[0], data[1]);
			break;

		case CMD_GET_ANGLE:
//			io_board_as5047_angle = buffer_get_float32(data, 1e3, &ind);
			// io_board_as5047_angle = buffer_get_float32_auto(data, &ind);
			break;

		// ==================== vehicle commands ==================== //
		case CMD_GET_STATE: {
			timeout_reset();

			POS_STATE pos, pos_uwb;
			mc_values mcval;
			float accel[3];
			float gyro[3];
			float mag[3];
			ROUTE_POINT rp_goal;

			commands_set_send_func(func);

			pos_get_imu(accel, gyro, mag);
			pos_get_pos(&pos);
			pos_get_mc_val(&mcval);
			autopilot_get_goal_now(&rp_goal);
			pos_uwb_get_pos(&pos_uwb);

			fi_inject_fault_float("px", &pos.px);

			int32_t send_index = 0;
			m_send_buffer[send_index++] = id_ret; // 1
			m_send_buffer[send_index++] = CMD_GET_STATE; // 2
			m_send_buffer[send_index++] = FW_VERSION_MAJOR; // 3
			m_send_buffer[send_index++] = FW_VERSION_MINOR; // 4
			buffer_append_float32(m_send_buffer, pos.roll, 1e6, &send_index); // 8
			buffer_append_float32(m_send_buffer, pos.pitch, 1e6, &send_index); // 12
			if (main_config.vehicle.use_uwb_pos) {
				buffer_append_float32(m_send_buffer, pos_uwb.yaw, 1e6, &send_index); // 16
			} else {
				buffer_append_float32(m_send_buffer, pos.yaw, 1e6, &send_index); // 16
			}
//			commands_printf("Sending yaw: %f\n",pos.yaw);

			buffer_append_float32(m_send_buffer, accel[0], 1e6, &send_index); // 20
			buffer_append_float32(m_send_buffer, accel[1], 1e6, &send_index); // 24
			buffer_append_float32(m_send_buffer, accel[2], 1e6, &send_index); // 28
			buffer_append_float32(m_send_buffer, gyro[0], 1e6, &send_index); // 32
			buffer_append_float32(m_send_buffer, gyro[1], 1e6, &send_index); // 36
			buffer_append_float32(m_send_buffer, gyro[2], 1e6, &send_index); // 40
			buffer_append_float32(m_send_buffer, mag[0], 1e6, &send_index); // 44
			buffer_append_float32(m_send_buffer, mag[1], 1e6, &send_index); // 48
			buffer_append_float32(m_send_buffer, mag[2], 1e6, &send_index); // 52
			if (main_config.vehicle.use_uwb_pos) {
				buffer_append_float32(m_send_buffer, pos_uwb.px, 1e4, &send_index); // 56
				buffer_append_float32(m_send_buffer, pos_uwb.py, 1e4, &send_index); // 60
			} else {
				buffer_append_float32(m_send_buffer, pos.px, 1e4, &send_index); // 56
				buffer_append_float32(m_send_buffer, pos.py, 1e4, &send_index); // 60
			}
			buffer_append_float32(m_send_buffer, pos.speed, 1e6, &send_index); // 64
			#ifdef USE_ADCONV_FOR_VIN
			buffer_append_float32(m_send_buffer, adconv_get_vin(), 1e6, &send_index); // 68
			#else
			buffer_append_float32(m_send_buffer, mcval.v_in, 1e6, &send_index); // 68
			#endif
//			buffer_append_float32(m_send_buffer, debugvalue, 1e6, &send_index); // 111

			buffer_append_float32(m_send_buffer, mcval.temp_mos, 1e6, &send_index); // 72
			m_send_buffer[send_index++] = mcval.fault_code; // 73
			buffer_append_float32(m_send_buffer, pos.px_gps, 1e4, &send_index); // 77
			buffer_append_float32(m_send_buffer, pos.py_gps, 1e4, &send_index); // 81
			buffer_append_float32(m_send_buffer, rp_goal.px, 1e4, &send_index); // 85
			buffer_append_float32(m_send_buffer, rp_goal.py, 1e4, &send_index); // 89
			buffer_append_float32(m_send_buffer, autopilot_get_rad_now(), 1e6, &send_index); // 93
			buffer_append_int32(m_send_buffer, pos_get_ms_today(), &send_index); // 97
			buffer_append_int16(m_send_buffer, autopilot_get_route_left(), &send_index); // 99
			if (main_config.vehicle.use_uwb_pos) {
				buffer_append_float32(m_send_buffer, pos.px, 1e4, &send_index); // 103
				buffer_append_float32(m_send_buffer, pos.py, 1e4, &send_index); // 107
			} else {
				buffer_append_float32(m_send_buffer, pos_uwb.px, 1e4, &send_index); // 103
				buffer_append_float32(m_send_buffer, pos_uwb.py, 1e4, &send_index); // 107
			}
			buffer_append_float32(m_send_buffer, debugvalue, 1e4, &send_index); // 111
			buffer_append_float32(m_send_buffer, debugvalue, 1e4, &send_index); // 115
			buffer_append_float32(m_send_buffer, debugvalue, 1e4, &send_index); // 119 */
			buffer_append_float32(m_send_buffer, io_board_as5047_angle, 1e4, &send_index); // 111
			buffer_append_float32(m_send_buffer, servo_output, 1e4, &send_index); // 115
			buffer_append_uint16(m_send_buffer, last_sensorvalue, &send_index); // 119 */
			buffer_append_float32(m_send_buffer, debugvalue, 1e4, &send_index); // 121
			buffer_append_float32(m_send_buffer, debugvalue2, 1e4, &send_index); // 125
			buffer_append_float32(m_send_buffer, debugvalue3, 1e4, &send_index); // 129
			buffer_append_float32(m_send_buffer, debugvalue4, 1e4, &send_index); // 133
			buffer_append_float32(m_send_buffer, debugvalue5, 1e4, &send_index); // 137
			buffer_append_float32(m_send_buffer, debugvalue6, 1e4, &send_index); // 141
			buffer_append_float32(m_send_buffer, debugvalue7, 1e4, &send_index); // 145
			buffer_append_float32(m_send_buffer, debugvalue8, 1e4, &send_index); // 149
			buffer_append_float32(m_send_buffer, debugvalue9, 1e4, &send_index); // 153
			buffer_append_float32(m_send_buffer, debugvalue10, 1e4, &send_index); // 157
			buffer_append_float32(m_send_buffer, debugvalue11, 1e4, &send_index); // 161
			buffer_append_float32(m_send_buffer, debugvalue12, 1e4, &send_index); // 165
			buffer_append_float32(m_send_buffer, debugvalue13, 1e4, &send_index); // 169
			buffer_append_float32(m_send_buffer, debugvalue14, 1e4, &send_index); // 173
			buffer_append_float32(m_send_buffer, debugvalue15, 1e4, &send_index); // 177
			commands_send_packet(m_send_buffer, send_index);
		} break;

		case CMD_GET_VESC_STATUS: {
			// Läser bara: listar de VESC som skickat CAN-status (comm_can.c),
			// ändrar inget och skickar inget på CAN-bussen.
			// Svar: [id][cmd] { [vesc_id u8][ålder ms u16][rpm i32]
			//                   [ström*10 i16][duty*1000 i16] } för varje VESC.
			commands_set_send_func(func);

			int32_t send_index = 0;
			m_send_buffer[send_index++] = id_ret;
			m_send_buffer[send_index++] = packet_id;

			for (int i = 0;;i++) {
				can_status_msg *msg = comm_can_get_status_msg_index(i);
				if (!msg) {
					break; // slut på lagrade statusplatser
				}
				if (msg->id < 0 || msg->id > 255) {
					continue; // oanvänd plats (id = -1)
				}

				uint32_t age_ms = ST2MS(chVTTimeElapsedSinceX(msg->rx_time));
				if (age_ms > 65535) {
					age_ms = 65535;
				}

				m_send_buffer[send_index++] = (uint8_t)msg->id;
				buffer_append_uint16(m_send_buffer, (uint16_t)age_ms, &send_index);
				buffer_append_int32(m_send_buffer, (int32_t)msg->rpm, &send_index);
				buffer_append_int16(m_send_buffer, (int16_t)(msg->current * 10.0), &send_index);
				buffer_append_int16(m_send_buffer, (int16_t)(msg->duty * 1000.0), &send_index);
			}

			commands_send_packet(m_send_buffer, send_index);
		} break;

		case CMD_PING_VESC_CAN: {
			// Pingar ID 0-253 på CAN (som VESC Tool) och svarar med de som
			// svarade: [id][cmd] { [vesc_id u8] } för varje VESC.
			commands_set_send_func(func);

			uint8_t ids[64];
			int n = comm_can_ping_vescs(ids, sizeof(ids));

			int32_t send_index = 0;
			m_send_buffer[send_index++] = id_ret;
			m_send_buffer[send_index++] = packet_id;
			for (int i = 0;i < n;i++) {
				m_send_buffer[send_index++] = ids[i];
			}
			commands_send_packet(m_send_buffer, send_index);
		} break;

		case CMD_GET_VINKEL: {
			// Bara läsning, för statusskärmen: ingen timeout_reset() (annars skulle
			// kortet aldrig stoppa om förarens länk dör) och ingen ny send_func
			// (Car_Client skickar allt kortet säger vidare till alla).
			// Svar: [id][cmd][status u8: 0 ingen givare, 1 OK, 2 fel][kalibrerad u8]
			//       [mV u16][grader*100 i32]
			int32_t send_index = 0;
			m_send_buffer[send_index++] = id_ret;
			m_send_buffer[send_index++] = CMD_GET_VINKEL;
#ifdef ANGLE_SENSOR_PA3
			m_send_buffer[send_index++] = adconv_angle_sensor_ok() ? 1 : 2;
			m_send_buffer[send_index++] = adconv_angle_calibrated() ? 1 : 0;
			buffer_append_uint16(m_send_buffer, last_sensorvalue, &send_index);
			buffer_append_int32(m_send_buffer, (int32_t)(frontangle * 100.0), &send_index);
#else
			m_send_buffer[send_index++] = 0;
			m_send_buffer[send_index++] = 0;
			buffer_append_uint16(m_send_buffer, 0, &send_index);
			buffer_append_int32(m_send_buffer, 0, &send_index);
#endif
			commands_send_packet(m_send_buffer, send_index);
		} break;

		case CMD_TEST_SPIN_VESC: {
			// [vesc_id u8] -> svar [id][cmd][vesc_id][1 = startat, 0 = upptaget]
			commands_set_send_func(func);
			if (len < 1) {
				break;
			}
			uint8_t vesc = data[0];
			bool started = motor_test_spin(vesc);
			commands_printf("Provsnurr VESC %d: %s", vesc, started ? "startat" : "upptaget");

			int32_t send_index = 0;
			m_send_buffer[send_index++] = id_ret;
			m_send_buffer[send_index++] = packet_id;
			m_send_buffer[send_index++] = vesc;
			m_send_buffer[send_index++] = started ? 1 : 0;
			commands_send_packet(m_send_buffer, send_index);
		} break;

		case CMD_VESC_FWD:
			timeout_reset();
			commands_set_send_func(func);

			bldc_interface_set_forward_func(commands_forward_vesc_packet);
			bldc_interface_send_packet(data, len);
			chVTSet(&vt, MS2ST(FWD_TIME), stop_forward, NULL);
			break;

		case CMD_RC_CONTROL: {
			timeout_reset();

			RC_MODE mode;
			float throttle, steering;
			int32_t ind = 0;
			mode = data[ind++];
			throttle = buffer_get_float32(data, 1e4, &ind);
			steering = buffer_get_float32(data, 1e6, &ind);
			debugvalue=steering;
			utils_truncate_number(&steering, -1.0, 1.0);

			//TODO: Could be an issue without speed sensor 
			// steering *= autopilot_get_steering_scale();

			autopilot_set_active(false);
			debugvalue=mode;
			motor_set_mode(mode);
			if (!main_config.vehicle.disable_motor) {
				utils_truncate_number(&throttle, -1.0, 1.0);
				if (iDebug==44)
				{
					iCounterCommands++;
					if (iCounterCommands == 50)
					{
						commands_printf("Mode: %i",RC_MODE_CURRENT);
						commands_printf("Throttle: %f",throttle);
						commands_printf("Steering: %f",steering);
						iCounterCommands=0;
					}
				}
				motor_set_throttle_and_steering(throttle,steering,frontangle);
			};
		};
			break;
		case CMD_RC_CONTROL_ADV: {
			timeout_reset();
			uint32_t activity;
			float magnitude;
			int32_t ind = 0;

			// Debug: Print raw buffer content
	/*		commands_printf("Buffer: %02X %02X %02X %02X %02X",
			              data[0], data[1], data[2], data[3], data[4]);*/

			activity = data[ind];
			ind += 1;
			// commands_printf("Activity: %d", activity); // Borttaget: floodade USB-debugutskriften på varje spakrörelse

			// Debug: Print raw int32 before conversion
			int32_t raw_value = buffer_get_int32(data, &ind);
//			commands_printf("Raw int32: %d (0x%08X)", raw_value, raw_value);

			magnitude = (float)raw_value / 1e4;  // Manual conversion for debug
	//		commands_printf("Magnitude: %f", magnitude);

			// RobAnt: drivsidor var för sig och vändning på stället
			if (motor_rc_adv((uint8_t)activity, magnitude)) {
				break;
			}

			// Get actuators with the specified activity
			int actuator_count = 0;
			ACTUATOR* actuators = motor_get_actuators_by_activity(activity, &actuator_count);
			commands_printf("Activity %d -> %d actuator(s)", activity, actuator_count);

			if (actuators != NULL) {
				// Process each actuator that matches the activity
				for (int i = 0; i < actuator_count; i++) {
					// Check if this actuator is a VESC motor (type = AT_VESCMOTOR = 0)

					if (actuators[i].type == AT_VESCMOTOR) {
						// Debug: Print actuator info and magnitude before calling VESC function
/*						commands_printf("Actuator %d: motorid=%d, mode=%d, type=%d, activity=%d",
						              i, actuators[i].motorid, actuators[i].mode, actuators[i].type, actuators[i].activity);
						commands_printf("Calling VESC with magnitude: %f", magnitude);*/
						
						// Styrningen med vinkelgivare: spakens utslag = hjulens vinkel
						// (styr_position.c). Annars som förut: värdet direkt till VESC:en.
						if (activity == STYRPOS_ACTIVITY && styrpos_command(actuators[i].motorid, magnitude)) {
							continue;
						}
						// Use the actuator's mode and VESC ID to control it
						motor_set_vesc_value(actuators[i].motorid, magnitude, actuators[i].mode);
					} else
					{
						commands_printf("Hydraulic command!");
				//		hydraulic_move(actuators[i].motorid,magnitude);
					}
					// TODO: Add support for hydraulic actuators if needed
				}
				free(actuators); // Free the allocated memory
			}
		}
			break;

		case CMD_SET_SERVO_DIRECT: {
			timeout_reset();
	//		if ((io_board_as5047_angle>-30) && (io_board_as5047_angle<30))
	//		{

			int32_t ind = 0;
			float steering = buffer_get_float32(data, 1e6, &ind);
			utils_truncate_number(&steering, 0.0, 1.0);
			servo_simple_set_pos_ramp(steering, true);
		} break;

		// Sensor control commands
		case CMD_GET_SENSOR_VALUE: {
			timeout_reset();
			commands_set_send_func(func);

			int32_t ind = 0;
			uint16_t sensorid = buffer_get_uint16(data, &ind);
			uint16_t type = buffer_get_uint16(data, &ind);

			float value = sensor_read_value(sensorid, type);

			int32_t send_index = 0;
			m_send_buffer[send_index++] = id_ret;
			m_send_buffer[send_index++] = CMD_GET_SENSOR_VALUE;
			buffer_append_float32(m_send_buffer, value, 1e4, &send_index);
			commands_send_packet(m_send_buffer, send_index);
		} break;

		case CMD_GET_SENSOR_BY_ACTIVITY: {
			timeout_reset();
			commands_set_send_func(func);

			int32_t ind = 0;
			uint16_t activity = buffer_get_uint16(data, &ind);

			float value = sensor_get_activity_value(activity);

			int32_t send_index = 0;
			m_send_buffer[send_index++] = id_ret;
			m_send_buffer[send_index++] = CMD_GET_SENSOR_BY_ACTIVITY;
			buffer_append_float32(m_send_buffer, value, 1e4, &send_index);
			commands_send_packet(m_send_buffer, send_index);
		} break;

		case CMD_SET_SENSOR_CONFIG: {
			timeout_reset();
			commands_set_send_func(func);

			int32_t ind = 0;
			MAIN_CONFIG conf;
			conf_general_read_main_conf(&conf);

			conf.vehicle.sensors = buffer_get_uint16(data, &ind);
			for (int i = 0; i < 4; i++) {
				conf.vehicle.sensor[i].type = buffer_get_uint16(data, &ind);
				conf.vehicle.sensor[i].sensorid = buffer_get_uint16(data, &ind);
				conf.vehicle.sensor[i].activity = buffer_get_uint16(data, &ind);
				conf.vehicle.sensor[i].reserved = buffer_get_uint16(data, &ind);
			}

			conf_general_store_main_config(&conf);

			int32_t send_index = 0;
			m_send_buffer[send_index++] = id_ret;
			m_send_buffer[send_index++] = packet_id;
			commands_send_packet(m_send_buffer, send_index);
		} break;

		case CMD_GET_SENSOR_CONFIG: {
			timeout_reset();
			commands_set_send_func(func);

			MAIN_CONFIG conf;
			conf_general_read_main_conf(&conf);

			int32_t send_index = 0;
			m_send_buffer[send_index++] = id_ret;
			m_send_buffer[send_index++] = packet_id;
			buffer_append_uint16(m_send_buffer, conf.vehicle.sensors, &send_index);

			for (int i = 0; i < 4; i++) {
				buffer_append_uint16(m_send_buffer, conf.vehicle.sensor[i].type, &send_index);
				buffer_append_uint16(m_send_buffer, conf.vehicle.sensor[i].sensorid, &send_index);
				buffer_append_uint16(m_send_buffer, conf.vehicle.sensor[i].activity, &send_index);
				buffer_append_uint16(m_send_buffer, conf.vehicle.sensor[i].reserved, &send_index);
			}

			commands_send_packet(m_send_buffer, send_index);
		} break;

		// State control commands
		case CMD_SET_STATE_CONTROL: {
			timeout_reset();
			commands_set_send_func(func);

			int32_t ind = 0;
			uint16_t control_index = buffer_get_uint16(data, &ind);
			
			if (control_index < 4) {
				STATE_CONTROL* control = state_control_get_config(control_index);
				if (control) {
					control->actuator_activity = buffer_get_uint16(data, &ind);
					control->sensor_activity = buffer_get_uint16(data, &ind);
					control->control_type = buffer_get_uint16(data, &ind);
					control->target_value = buffer_get_float32(data, 1e4, &ind);
					control->kp = buffer_get_float32(data, 1e4, &ind);
					control->ki = buffer_get_float32(data, 1e4, &ind);
					control->kd = buffer_get_float32(data, 1e4, &ind);
					control->min_output = buffer_get_float32(data, 1e4, &ind);
					control->max_output = buffer_get_float32(data, 1e4, &ind);
					control->enabled = data[ind++];
				}
			}

			int32_t send_index = 0;
			m_send_buffer[send_index++] = id_ret;
			m_send_buffer[send_index++] = packet_id;
			commands_send_packet(m_send_buffer, send_index);
		} break;

		case CMD_GET_STATE_CONTROL: {
			timeout_reset();
			commands_set_send_func(func);

			int32_t ind = 0;
			uint16_t control_index = buffer_get_uint16(data, &ind);
			
			int32_t send_index = 0;
			m_send_buffer[send_index++] = id_ret;
			m_send_buffer[send_index++] = packet_id;
			
			if (control_index < 4) {
				STATE_CONTROL* control = state_control_get_config(control_index);
				if (control) {
					buffer_append_uint16(m_send_buffer, control->actuator_activity, &send_index);
					buffer_append_uint16(m_send_buffer, control->sensor_activity, &send_index);
					buffer_append_uint16(m_send_buffer, control->control_type, &send_index);
					buffer_append_float32(m_send_buffer, control->target_value, 1e4, &send_index);
					buffer_append_float32(m_send_buffer, control->kp, 1e4, &send_index);
					buffer_append_float32(m_send_buffer, control->ki, 1e4, &send_index);
					buffer_append_float32(m_send_buffer, control->kd, 1e4, &send_index);
					buffer_append_float32(m_send_buffer, control->min_output, 1e4, &send_index);
					buffer_append_float32(m_send_buffer, control->max_output, 1e4, &send_index);
					m_send_buffer[send_index++] = control->enabled;
				}
			}

			commands_send_packet(m_send_buffer, send_index);
		} break;

		case CMD_STATE_CONTROL_ENABLE: {
			timeout_reset();
			commands_set_send_func(func);

			int32_t ind = 0;
			uint16_t control_index = buffer_get_uint16(data, &ind);
			bool enabled = data[ind++];

			state_control_set_enabled(control_index, enabled);

			int32_t send_index = 0;
			m_send_buffer[send_index++] = id_ret;
			m_send_buffer[send_index++] = packet_id;
			commands_send_packet(m_send_buffer, send_index);
		} break;

		case CMD_STATE_CONTROL_TARGET: {
			timeout_reset();
			commands_set_send_func(func);

			int32_t ind = 0;
			uint16_t control_index = buffer_get_uint16(data, &ind);
			float target = buffer_get_float32(data, 1e4, &ind);

			state_control_set_target(control_index, target);

			int32_t send_index = 0;
			m_send_buffer[send_index++] = id_ret;
			m_send_buffer[send_index++] = packet_id;
			commands_send_packet(m_send_buffer, send_index);
		} break;

		// Test command
		case CMD_TEST_SENSOR_STATE: {
			timeout_reset();
			commands_set_send_func(func);

			test_sensor_state_control_init();

			int32_t send_index = 0;
			m_send_buffer[send_index++] = id_ret;
			m_send_buffer[send_index++] = packet_id;
			commands_send_packet(m_send_buffer, send_index);
		} break;

		default:
			break;
		}
	}
}

void commands_printf(const char* format, ...) {
	if (!m_init_done) {
		return;
	}

	chMtxLock(&m_print_gps);
	va_list arg;
	va_start (arg, format);
	int len;
	static char print_buffer[512];

	print_buffer[0] = main_id;
	print_buffer[1] = CMD_PRINTF;
	
	// Safety check: ensure we don't overflow the buffer
	len = vsnprintf(print_buffer + 2, 509, format, arg);
	va_end (arg);

	// Additional safety checks
	if(len < 0) {
		// vsnprintf failed - don't send anything
		chMtxUnlock(&m_print_gps);
		return;
	}

	if(len > 509) {
		// Buffer would be truncated - send maximum safe size
		len = 509;
	}

	if(len > 0) {
		// Ensure total packet size is reasonable
		int packet_len = len + 2;
		if(packet_len > 512) {
			packet_len = 512;
		}
		commands_send_packet((unsigned char*)print_buffer, packet_len);
	}
	chMtxUnlock(&m_print_gps);
}

void commands_printf_log_usb(char* format, ...) {
	va_list arg;
	va_start (arg, format);
	int len;
	static char print_buffer[255];

	print_buffer[0] = ID_VEHICLE_CLIENT;
	print_buffer[1] = CMD_LOG_LINE_USB;
	len = vsnprintf(print_buffer + 2, 253, format, arg);
	va_end (arg);

	if(len > 0) {
		comm_usb_send_packet((unsigned char*)print_buffer, (len<253) ? len + 2: 255);
	}
}

void commands_forward_vesc_packet(unsigned char *data, unsigned int len) {
	m_send_buffer[0] = main_id;
	m_send_buffer[1] = CMD_VESC_FWD;
	memcpy(m_send_buffer + 2, data, len);
	commands_send_packet((unsigned char*)m_send_buffer, len + 2);
}

void commands_send_nmea(unsigned char *data, unsigned int len) {
	// Egen buffert: anropas från u-blox-tråden och fick inte skriva i
	// m_send_buffer medan kommandotråden byggde ett svar där (t.ex. inställningarna).
	static uint8_t nmea_buffer[2 + 256];

	if (main_config.gps_send_nmea) {
		if (len > sizeof(nmea_buffer) - 2) {
			len = sizeof(nmea_buffer) - 2;
		}
		int32_t send_index = 0;
		nmea_buffer[send_index++] = main_id;
		nmea_buffer[send_index++] = CMD_SEND_NMEA_RADIO;
		memcpy(nmea_buffer + send_index, data, len);
		send_index += len;
		commands_send_packet(nmea_buffer, send_index);
	}
}

void commands_send_log_ethernet(unsigned char *data, int len) {
	int32_t ind = 0;
	m_send_buffer[ind++] = ID_VEHICLE_CLIENT;
	m_send_buffer[ind++] = CMD_LOG_ETHERNET;
	memcpy(m_send_buffer + ind, data, len);
	ind += len;
	comm_usb_send_packet(m_send_buffer, ind);
}

static void stop_forward(void *p) {
	(void)p;
	bldc_interface_set_forward_func(0);
}

static void rtcm_rx(uint8_t *data, int len, int type) {
	(void)type;

#if UBLOX_EN
	// We now send the raw RTCM data directly in CMD_SEND_RTCM_USB to prevent
	// message filtering/latency, so we do not send it again here.
	(void)data;
	(void)len;
	(void)m_send_buffer;
#else
	int32_t send_index = 0;
	m_send_buffer[send_index++] = main_id;
	m_send_buffer[send_index++] = CMD_SEND_RTCM_USB;
	memcpy(m_send_buffer + send_index, data, len);
	send_index += len;
	comm_usb_send_packet(m_send_buffer, send_index);
#endif
}

static void rtcm_base_rx(rtcm_ref_sta_pos_t *pos) {
	if (main_config.gps_use_rtcm_base_as_enu_ref) {
		pos_set_enu_ref(pos->lat, pos->lon, pos->height);
	}
}

rtcm3_state* commands_get_rtcm3_state(void) {
	return &m_rtcm_state;
}

void commands_sleep(void)
{

    chSysLock(); // Enter system lock to manipulate thread states safely
/*
    if (hydro_thread != NULL) {
        chThdTerminate(hydro_thread);
    }
    */
/*
    if (thread2 != NULL) {
        chThdTerminate(thread2);
    }*/

    chSysUnlock(); // Exit system lock
	// Signal all threads to terminate
//    chEvtBroadcastFlags(&emergency_event, EMERGENCY_STOP_EVENT);
}
