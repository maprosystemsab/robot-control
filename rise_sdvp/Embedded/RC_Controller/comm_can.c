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

#include <string.h>
#include "comm_can.h"
#include "ch.h"
#include "hal.h"
#include "stm32f4xx_conf.h"
#include "datatypes.h"
#include "buffer.h"
#include "crc.h"
#include "packet.h"
#include "bldc_interface.h"
#include "commands.h"
#include "motor_sim.h"

// CAN Communication Module
// This module implements CAN bus communication for the RC Controller system.
// It handles CAN message transmission and reception, VESC controller communication,
// IO board control, and various CAN-based sensors and actuators.
//
// Key Features:
// - CAN bus initialization and configuration
// - Thread-based CAN message processing
// - VESC (Vedder Electronic Speed Controller) communication
// - IO board ADC and angle sensor reading
// - ADDIO CAN protocol support
// - Proportional valve control (PVG32)
// - FTR2 sensor support
// - CAN message forwarding
// - Status message storage and retrieval
//
// The module uses a dual-thread architecture:
// 1. Read thread: Receives CAN messages and stores them in a circular buffer
// 2. Process thread: Processes received messages and executes appropriate actions

// Settings
#define CANDx						CAND1
#define RX_FRAMES_SIZE				100
#define RX_BUFFER_SIZE				PACKET_MAX_PL_LEN
#define CAN_STATUS_MSGS_TO_STORE	10
// Avsändar-ID när styrkortet pingar VESC (PONG skickas tillbaka till detta ID).
// 254 används inte av VESC i praktiken; 255 är broadcast.
#define CAN_PING_SENDER_ID			254

// ADDIO CAN (Additional IO CAN protocol)
// These are CAN IDs for ADDIO devices
#define CAN_ADDIO_MASTER			0x28
#define CAN_ANGLE					0x19F

// Threads
// The module uses two threads for CAN communication:
// 1. Read thread: Receives CAN messages from the bus and stores them in a buffer
// 2. Process thread: Processes the buffered messages and executes appropriate actions
static THD_WORKING_AREA(cancom_read_thread_wa, 512); // Read thread working area (512 bytes stack)
static THD_WORKING_AREA(cancom_process_thread_wa, 4096); // Process thread working area (4096 bytes stack)
static THD_FUNCTION(cancom_read_thread, arg); // Read thread function
static THD_FUNCTION(cancom_process_thread, arg); // Process thread function

extern int iDebug; // External debug flag for conditional debug output

// Functions
//static void cmd_terminal_useeid(int argc, const char **argv); // Commented out: terminal command for EID filter

// Variables
static can_status_msg stat_msgs[CAN_STATUS_MSGS_TO_STORE]; // Status message storage for debugging
static mutex_t can_mtx; // Mutex for CAN bus access synchronization
static mutex_t vesc_mtx_ext; // Mutex for VESC access synchronization (external)
static uint8_t rx_buffer[RX_BUFFER_SIZE]; // Receive buffer for CAN message data
static unsigned int rx_buffer_last_id; // Last received CAN ID
static CANRxFrame rx_frames[RX_FRAMES_SIZE]; // Circular buffer for received CAN frames
static int rx_frame_read; // Read index for circular buffer
static int rx_frame_write; // Write index for circular buffer
static thread_t *process_tp; // Pointer to process thread
static int vesc_id = VESC_ID; // Current VESC controller ID (default: VESC_ID)
static volatile uint8_t pong_seen[32]; // Bit per VESC-ID som svarat på ping (comm_can_ping_vescs)

int iEid;
extern float servo_output;

// IO Board
// These variables store data received from the IO board via CAN bus
static float io_board_adc_voltages[8] = {0}; // ADC voltage readings from IO board (8 channels)
static bool io_board_lim_sw[8] = {0}; // Limit switch states from IO board (8 switches)
//static float io_board_as5047_angle = 0.0; // Commented out: AS5047 angle sensor value (static?)
float io_board_as5047_angle = 0.0; // AS5047 angle sensor value (degrees) - made global for external access
static float can_ftr2_angle = 0.0; // FTR2 sensor angle value (degrees)

#ifndef SERVO_READ
ADC_CNT_t io_board_adc0_cnt = {1};
#endif

extern int iDebug;

// ADDIO (Additional Digital IO)
// Variables for ADDIO CAN protocol support
static bool addio_lim_sw[8] = {1}; // ADDIO limit switch states (8 switches, initialized to true)
static uint8_t pvg32_node_id = 0; // PVG32 proportional valve node ID
static bool ftr2_activated = false; // Flag indicating if FTR2 sensor is activated
static int ftr2_id = 0x61F; // FTR2 sensor CAN ID
static uint8_t ftr2_frame[8] = {0x2f, 0x00, 0x18, 0x02, 0xFE, 0x00, 0x00, 0x00}; // FTR2 frame data

#ifdef ADDIO
// CAN configuration for ADDIO mode
/*
 * 512.5KBaud, automatic wakeup, automatic recover
 * from abort mode.
 * See section 22.7.7 on the STM32 reference manual.
 */
static const CANConfig cancfg = {
		CAN_MCR_ABOM | CAN_MCR_AWUM | CAN_MCR_TXFP, // Automatic bus-off management, automatic wakeup, transmit FIFO priority
		CAN_BTR_SJW(0) | CAN_BTR_TS2(1) | // Synchronization jump width = 0, Time segment 2 = 1
		CAN_BTR_TS1(8) | CAN_BTR_BRP(27) // Time segment 1 = 8, Baud rate prescaler = 27
};
#else
// CAN configuration for standard mode
/*
 * 500KBaud, automatic wakeup, automatic recover
 * from abort mode.
 * See section 22.7.7 on the STM32 reference manual.
 */
static const CANConfig cancfg = {
		CAN_MCR_ABOM | CAN_MCR_AWUM | CAN_MCR_TXFP, // Automatic bus-off management, automatic wakeup, transmit FIFO priority
		CAN_BTR_SJW(0) | CAN_BTR_TS2(1) | // Synchronization jump width = 0, Time segment 2 = 1
		CAN_BTR_TS1(8) | CAN_BTR_BRP(6) // Time segment 1 = 8, Baud rate prescaler = 6
};
#endif

// Private functions
static void send_packet_wrapper(unsigned char *data, unsigned int len); // Wrapper for sending CAN packets
static void printf_wrapper(char *str); // Wrapper for printf output
static uint8_t update_addio_outputs(int valve, bool set); // Update ADDIO outputs (valve control)
static void prop_valve_nmt_sm(uint8_t node_id, uint8_t state); // Proportional valve NMT (Network Management) state machine
static void prop_valve_status_sm(uint8_t node_id, uint8_t* data); // Proportional valve status state machine

// Function pointers for callbacks
static void(*m_range_func)(uint8_t id, uint8_t dest, float range) = 0; // Callback for range measurements
static void(*m_dw_ping_func)(uint8_t id) = 0; // Callback for distance sensor ping
static void(*m_dw_uptime_func)(uint8_t id, uint32_t uptime) = 0; // Callback for distance sensor uptime

// Initialize the CAN communication module
// This function initializes all CAN-related hardware, threads, and data structures.
void comm_can_init(void) {
	// Initialize status message storage
	for (int i = 0;i < CAN_STATUS_MSGS_TO_STORE;i++) {
		stat_msgs[i].id = -1; // Mark as unused
	}

	// Initialize circular buffer indices
	rx_frame_read = 0; // Read index
	rx_frame_write = 0; // Write index
	
	// Initialize VESC ID
	vesc_id = VESC_ID; // Set to default VESC ID

	// Commented out: Terminal command for EID filter (debug feature)
/*
	terminal_register_command_callback(
			"eid",
			"Set value for eid filter for debug info",
			0,
			cmd_terminal_useeid);
*/

	// Initialize mutexes for thread safety
	chMtxObjectInit(&can_mtx); // CAN bus mutex
	chMtxObjectInit(&vesc_mtx_ext); // VESC mutex

	// Configure CAN GPIO pins for alternate function (CAN1)
	palSetPadMode(CAN1_RX_GPIO, CAN1_RX_PIN, PAL_MODE_ALTERNATE(GPIO_AF_CAN1));
	palSetPadMode(CAN1_TX_GPIO, CAN1_TX_PIN, PAL_MODE_ALTERNATE(GPIO_AF_CAN1));

	// Start the CAN interface with the configured settings
	canStart(&CANDx, &cancfg);

	// Initialize BLDC interface (unless in ADDIO mode)
	#ifndef ADDIO
	bldc_interface_init(send_packet_wrapper); // Register send wrapper
	bldc_interface_set_rx_printf_func(printf_wrapper); // Register printf wrapper
	#endif
	
	// Create CAN communication threads
	chThdCreateStatic(cancom_read_thread_wa, sizeof(cancom_read_thread_wa), NORMALPRIO + 1,
			cancom_read_thread, NULL); // Read thread with higher priority
	chThdCreateStatic(cancom_process_thread_wa, sizeof(cancom_process_thread_wa), NORMALPRIO,
			cancom_process_thread, NULL); // Process thread with normal priority
}

// Set the current VESC controller ID
// This function changes which VESC controller the module communicates with.
//
// @param id The VESC ID to set (VESC_LEFT, VESC_RIGHT, or other VESC_ID values)
void comm_can_set_vesc_id(int id) {
	// Debug output for motor selection
	if (iDebug==43)
	{
		commands_printf("motor: %i",id);
	}

	vesc_id = id; // Update the current VESC ID

	// Update motor simulation based on VESC ID
	if (vesc_id == VESC_LEFT) {
		motor_sim_set_motor(0); // Set to left motor
	} else if (vesc_id == VESC_RIGHT) {
		motor_sim_set_motor(1); // Set to right motor
	}
}

// Lock the VESC mutex
// This function locks the VESC mutex to ensure thread-safe access to VESC resources.
// Must be paired with comm_can_unlock_vesc().
void comm_can_lock_vesc(void) {
	chMtxLock(&vesc_mtx_ext); // Lock the VESC mutex
}

// Unlock the VESC mutex
// This function unlocks the VESC mutex after a critical section.
// Must be paired with comm_can_lock_vesc().
void comm_can_unlock_vesc(void) {
	chMtxUnlock(&vesc_mtx_ext); // Unlock the VESC mutex
}

static THD_FUNCTION(cancom_read_thread, arg) {
	(void)arg;
	chRegSetThreadName("CAN read");

	event_listener_t el;
	CANRxFrame rxmsg;

	chEvtRegister(&CANDx.rxfull_event, &el, 0);

	while(!chThdShouldTerminateX()) {
		if (chEvtWaitAnyTimeout(ALL_EVENTS, MS2ST(10)) == 0) {
			continue;
		}

		msg_t result = canReceive(&CANDx, CAN_ANY_MAILBOX, &rxmsg, TIME_IMMEDIATE);

		while (result == MSG_OK) {
			rx_frames[rx_frame_write++] = rxmsg;
			if (rx_frame_write == RX_FRAMES_SIZE) {
				rx_frame_write = 0;
			}

			chEvtSignal(process_tp, (eventmask_t) 1);

			result = canReceive(&CANDx, CAN_ANY_MAILBOX, &rxmsg, TIME_IMMEDIATE);
		}
	}

	chEvtUnregister(&CANDx.rxfull_event, &el);
}

/*
 * void find_ones_positions(unsigned long number) {
    unsigned long mask = 1;
    int position = 1;

    commands_printf("Positions of '1's in the binary representation of %lu are: ", number);

    while (number > 0) {
        if (number & mask) {
        	commands_printf("%d ", position);
        }
        number >>= 1;
        position++;
    }
    commands_printf("\n");
}*/

/*
static void cmd_terminal_useeid(int argc, const char **argv) {
	sscanf(argv[1], "%i", &iEid);
	commands_printf("Eid: %i\n",iEid);
}
*/

static THD_FUNCTION(cancom_process_thread, arg) {
	(void)arg;

	chRegSetThreadName("CAN process");
	process_tp = chThdGetSelfX();

	int32_t ind = 0;
	unsigned int rxbuf_len;
	unsigned int rxbuf_ind;
	uint8_t crc_low;
	uint8_t crc_high;
	bool commands_send;

	for(;;) {
		chEvtWaitAny((eventmask_t) 1);

		while (rx_frame_read != rx_frame_write) {
			CANRxFrame rxmsg = rx_frames[rx_frame_read++];
			if (rxmsg.IDE == CAN_IDE_EXT) {
				// Process extended IDs (VESC Communication)
				uint8_t id = rxmsg.EID & 0xFF;
				CAN_PACKET_ID cmd = rxmsg.EID >> 8;
				can_status_msg *stat_tmp;

				// VESC-svar (buffertpaketen) läses bara om de är adresserade till
				// styrkortet (main_id + 128, se comm_can_send_buffer). Annars tolkas
				// t.ex. VESC Tools trafik på bussen som våra svar -> 0 V och
				// okänd felkod i RControlStation.
				if ((cmd == CAN_PACKET_FILL_RX_BUFFER || cmd == CAN_PACKET_FILL_RX_BUFFER_LONG ||
						cmd == CAN_PACKET_PROCESS_RX_BUFFER || cmd == CAN_PACKET_PROCESS_SHORT_BUFFER) &&
						id != (uint8_t)(main_id + 128)) {
					continue;
				}

				switch (cmd) {
				case CAN_PACKET_FILL_RX_BUFFER:
					memcpy(rx_buffer + rxmsg.data8[0], rxmsg.data8 + 1, rxmsg.DLC - 1);
					break;

				case CAN_PACKET_FILL_RX_BUFFER_LONG:
					rxbuf_ind = (unsigned int)rxmsg.data8[0] << 8;
					rxbuf_ind |= rxmsg.data8[1];
					if (rxbuf_ind < RX_BUFFER_SIZE) {
						memcpy(rx_buffer + rxbuf_ind, rxmsg.data8 + 2, rxmsg.DLC - 2);
					}
					break;

				case CAN_PACKET_PROCESS_RX_BUFFER:
					ind = 0;
					rx_buffer_last_id = rxmsg.data8[ind++];
					commands_send = rxmsg.data8[ind++];
					rxbuf_len = (unsigned int)rxmsg.data8[ind++] << 8;
					rxbuf_len |= (unsigned int)rxmsg.data8[ind++];

					if (rxbuf_len > RX_BUFFER_SIZE) {
						break;
					}

					crc_high = rxmsg.data8[ind++];
					crc_low = rxmsg.data8[ind++];

					if (crc16(rx_buffer, rxbuf_len)
							== ((unsigned short) crc_high << 8
									| (unsigned short) crc_low)) {

						(void)commands_send;
						bldc_interface_process_packet(rx_buffer, rxbuf_len);
					}
					break;

				case CAN_PACKET_PROCESS_SHORT_BUFFER:
					ind = 0;
					rx_buffer_last_id = rxmsg.data8[ind++];
					commands_send = rxmsg.data8[ind++];
					(void)commands_send;
					bldc_interface_process_packet(rxmsg.data8 + ind, rxmsg.DLC - ind);
					break;

				case CAN_PACKET_STATUS:
					for (int i = 0;i < CAN_STATUS_MSGS_TO_STORE;i++) {
						stat_tmp = &stat_msgs[i];
						if (stat_tmp->id == id || stat_tmp->id == -1) {
							ind = 0;
							stat_tmp->id = id;
							stat_tmp->rx_time = chVTGetSystemTime();
							stat_tmp->rpm = (float)buffer_get_int32(rxmsg.data8, &ind);
							stat_tmp->current = (float)buffer_get_int16(rxmsg.data8, &ind) / 10.0;
							stat_tmp->duty = (float)buffer_get_int16(rxmsg.data8, &ind) / 1000.0;
							break;
						}
					}
					break;

				case CAN_PACKET_PONG:
					// Svar på vår ping: data[0] är VESC:ns eget ID.
					if (id == CAN_PING_SENDER_ID && rxmsg.DLC >= 1) {
						uint8_t from = rxmsg.data8[0];
						pong_seen[from >> 3] |= (uint8_t)(1 << (from & 7));
					}
					break;

				default:
					break;
				}
			} else if (rxmsg.IDE == CAN_IDE_STD) {
				// Process standard IDs
#if CAN_EN_DW
				if ((rxmsg.SID & 0x700) == CAN_MASK_DW) {
					switch (rxmsg.data8[0]) {
					case CMD_DW_RANGE: {
						int32_t ind = 1;
						uint8_t id = rxmsg.SID & 0xFF;
						uint8_t dest = rxmsg.data8[ind++];
						float range = (float)buffer_get_int32(rxmsg.data8, &ind) / 1000.0;

						if (m_range_func) {
							m_range_func(id, dest, range);
						}
					} break;

					case CMD_DW_PING: {
						if (m_dw_ping_func) {
							uint8_t id = rxmsg.SID & 0xFF;
							m_dw_ping_func(id);
						}
					} break;

					case CMD_DW_UPTIME: {
						int32_t ind = 1;
						uint8_t id = rxmsg.SID & 0xFF;
						uint32_t uptime = buffer_get_uint32(rxmsg.data8, &ind);

						if (m_dw_uptime_func) {
							m_dw_uptime_func(id, uptime);
						}
					} break;

					default:
						break;
					}
				}
#endif
#if CAN_IO_BOARD
				if ((rxmsg.SID & 0x700) == CAN_MASK_IO_BOARD) {
//					uint16_t id = rxmsg.SID & 0x0F;
					uint16_t msg = (rxmsg.SID >> 4) & 0x0F;
					int32_t ind = 0;

					switch (msg) {
/*					case CAN_IO_PACKET_ADC_VOLTAGES_0_1_2_3:
						ind = 0;
						io_board_adc_voltages[0] = buffer_get_float16(rxmsg.data8, 0.5e3, &ind);
						io_board_adc_voltages[1] = buffer_get_float16(rxmsg.data8, 0.5e3, &ind);
						io_board_adc_voltages[2] = buffer_get_float16(rxmsg.data8, 0.5e3, &ind);
						io_board_adc_voltages[3] = buffer_get_float16(rxmsg.data8, 0.5e3, &ind);

						if (io_board_adc_voltages[0] >= 9.9) {
							io_board_adc0_cnt.is_high = true;
						} else if (io_board_adc_voltages[0] < 3.3) {
							io_board_adc0_cnt.is_high = false;
						}
						break;
*/
					case CAN_IO_PACKET_ADC_VOLTAGES_4_5_6_7:
						ind = 0;
						io_board_adc_voltages[4] = buffer_get_float16(rxmsg.data8, 0.5e3, &ind);
						io_board_adc_voltages[5] = buffer_get_float16(rxmsg.data8, 0.5e3, &ind);
						io_board_adc_voltages[6] = buffer_get_float16(rxmsg.data8, 0.5e3, &ind);
						io_board_adc_voltages[7] = buffer_get_float16(rxmsg.data8, 0.5e3, &ind);
						break;
#ifndef ANALOG_ANGLE
					case CAN_IO_PACKET_AS5047_ANGLE:
						ind = 0;
						io_board_as5047_angle = buffer_get_float32(rxmsg.data8, 1e3, &ind);
						break;
#endif
					case CAN_IO_PACKET_LIM_SW:
						for (int i = 0;i < rxmsg.DLC; i++) {
							io_board_lim_sw[i] = rxmsg.data8[i];
						}
						break;

					case CAN_IO_PACKET_ADC0_HIGH_TIME:
						ind = 0;
						io_board_adc0_cnt.high_time_last = buffer_get_float32_auto(rxmsg.data8, &ind);
						io_board_adc0_cnt.high_time_current = buffer_get_float32_auto(rxmsg.data8, &ind);
						break;

					case CAN_IO_PACKET_ADC0_LOW_TIME:
						ind = 0;
						io_board_adc0_cnt.low_time_last = buffer_get_float32_auto(rxmsg.data8, &ind);
						io_board_adc0_cnt.low_time_current = buffer_get_float32_auto(rxmsg.data8, &ind);
						break;

					case CAN_IO_PACKET_ADC0_HIGH_LOW_CNT:
						ind = 0;
						io_board_adc0_cnt.toggle_high_cnt = buffer_get_uint32(rxmsg.data8, &ind);
						io_board_adc0_cnt.toggle_low_cnt = buffer_get_uint32(rxmsg.data8, &ind);
						break;

					default:
						break;
					}
				}
#endif
#if CAN_ADDIO
				if (rxmsg.SID == CAN_ANGLE) {
					ftr2_activated = true;
//					can_ftr2_angle = (((rxmsg.data8[1] << 8) | rxmsg.data8[0]) / 10) + 1.7;
					can_ftr2_angle = (((rxmsg.data8[1] << 8) | rxmsg.data8[0]) / 10) + 1.0;
//					can_ftr2_angle=can_ftr2_angle+2.0; vänster
//					can_ftr2_angle=can_ftr2_angle+1.6; höger
					//rough conversion to angles based on test with the MacTrac
				}
				if ((rxmsg.SID & 0x700) == CAN_MASK_PVG32 && rxmsg.SID != 0x71F) {
					prop_valve_nmt_sm(rxmsg.SID - 0x700, rxmsg.data8[0]); 
				}
				if (rxmsg.SID == (0x180 + pvg32_node_id)) {
					prop_valve_status_sm(pvg32_node_id, rxmsg.data8);
				}
				if (rxmsg.SID == CAN_ADDIO_MASTER) {
					for (int i = 0;i < rxmsg.DLC; i++) {
						addio_lim_sw[i] = (rxmsg.data8[0] >> i) & 0x01;
					}
				}
#endif
			}

			if (rx_frame_read == RX_FRAMES_SIZE) {
				rx_frame_read = 0;
			}
		}
	}
}


void comm_can_transmit_eid(uint32_t id, uint8_t *data, uint8_t len) {
	CANTxFrame txmsg;
	txmsg.IDE = CAN_IDE_EXT;
	txmsg.EID = id;
	txmsg.RTR = CAN_RTR_DATA;
	txmsg.DLC = len;
	memcpy(txmsg.data8, data, len);

	chMtxLock(&can_mtx);
	canTransmit(&CANDx, CAN_ANY_MAILBOX, &txmsg, MS2ST(20));
	chMtxUnlock(&can_mtx);
}

void comm_can_transmit_sid(uint32_t id, uint8_t *data, uint8_t len) {
	CANTxFrame txmsg;
	txmsg.IDE = CAN_IDE_STD;
	txmsg.SID = id;
	txmsg.RTR = CAN_RTR_DATA;
	txmsg.DLC = len;
	memcpy(txmsg.data8, data, len);

	chMtxLock(&can_mtx);
	canTransmit(&CANDx, CAN_ANY_MAILBOX, &txmsg, MS2ST(20));
	chMtxUnlock(&can_mtx);
}

/**
 * Send a buffer up to RX_BUFFER_SIZE bytes as fragments. If the buffer is 6 bytes or less
 * it will be sent in a single CAN frame, otherwise it will be split into
 * several frames.
 *
 * @param controller_id
 * The controller id to send to.
 *
 * @param data
 * The payload.
 *
 * @param len
 * The payload length.
 *
 * @param send
 * If true, this packet will be passed to the send function of commands.
 * Otherwise, it will be passed to the process function (DON'T CARE HERE, only for VESC).
 */
void comm_can_send_buffer(uint8_t controller_id, uint8_t *data, unsigned int len, bool send) {
	uint8_t send_buffer[8];

	if (len <= 6) {
		uint32_t ind = 0;
		send_buffer[ind++] = main_id + 128;
		send_buffer[ind++] = send;
		memcpy(send_buffer + ind, data, len);
		ind += len;
		comm_can_transmit_eid(controller_id | ((uint32_t)CAN_PACKET_PROCESS_SHORT_BUFFER << 8), send_buffer, ind);
	} else {
		unsigned int end_a = 0;
		for (unsigned int i = 0;i < len;i += 7) {
			if (i > 255) {
				break;
			}

			end_a = i + 7;

			uint8_t send_len = 7;
			send_buffer[0] = i;

			if ((i + 7) <= len) {
				memcpy(send_buffer + 1, data + i, send_len);
			} else {
				send_len = len - i;
				memcpy(send_buffer + 1, data + i, send_len);
			}

			comm_can_transmit_eid(controller_id | ((uint32_t)CAN_PACKET_FILL_RX_BUFFER << 8), send_buffer, send_len + 1);
		}

		for (unsigned int i = end_a;i < len;i += 6) {
			uint8_t send_len = 6;
			send_buffer[0] = i >> 8;
			send_buffer[1] = i & 0xFF;

			if ((i + 6) <= len) {
				memcpy(send_buffer + 2, data + i, send_len);
			} else {
				send_len = len - i;
				memcpy(send_buffer + 2, data + i, send_len);
			}

			comm_can_transmit_eid(controller_id | ((uint32_t)CAN_PACKET_FILL_RX_BUFFER_LONG << 8), send_buffer, send_len + 2);
		}

		uint32_t ind = 0;
		send_buffer[ind++] = main_id + 128;
		send_buffer[ind++] = send;
		send_buffer[ind++] = len >> 8;
		send_buffer[ind++] = len & 0xFF;
		unsigned short crc = crc16(data, len);
		send_buffer[ind++] = (uint8_t)(crc >> 8);
		send_buffer[ind++] = (uint8_t)(crc & 0xFF);

		comm_can_transmit_eid(controller_id | ((uint32_t)CAN_PACKET_PROCESS_RX_BUFFER << 8), send_buffer, ind++);
	}
}

/**
 * Start ranging with a DW node on the CAN bus.
 *
 * @param id
 * The ID of the DW node.
 *
 * @param dest
 * The ID of the node to range to.
 *
 * @param samples
 * How many samples to average over.
 */
void comm_can_dw_range(uint8_t id, uint8_t dest, int samples) {
	uint8_t buffer[8];
	int32_t ind = 0;

	buffer[ind++] = CMD_DW_RANGE;
	buffer[ind++] = dest;
	buffer[ind++] = samples;

	comm_can_transmit_sid(((uint32_t)id | CAN_MASK_DW), buffer, ind);
}

void comm_can_dw_ping(uint8_t id) {
	uint8_t buffer[8];
	int32_t ind = 0;
	buffer[ind++] = CMD_DW_PING;
	comm_can_transmit_sid(((uint32_t)id | CAN_MASK_DW), buffer, ind);
}

void comm_can_dw_reboot(uint8_t id) {
	uint8_t buffer[8];
	int32_t ind = 0;
	buffer[ind++] = CMD_DW_REBOOT;
	comm_can_transmit_sid(((uint32_t)id | CAN_MASK_DW), buffer, ind);
}

void comm_can_dw_get_uptime(uint8_t id) {
	uint8_t buffer[8];
	int32_t ind = 0;
	buffer[ind++] = CMD_DW_UPTIME;
	comm_can_transmit_sid(((uint32_t)id | CAN_MASK_DW), buffer, ind);
}

/**
 * Set the function to be called when ranging is done.
 *
 * @param func
 * A pointer to the function.
 */
void comm_can_set_range_func(void(*func)(uint8_t id, uint8_t dest, float range)) {
	m_range_func = func;
}

/**
 * Set the function to be called when a ping is received from the UWB board.
 *
 * @param func
 * A pointer to the function.
 */
void comm_can_set_dw_ping_func(void(*func)(uint8_t uptime)) {
	m_dw_ping_func = func;
}

/**
 * Set the function to be called when a uptime result is received from the UWB board.
 *
 * @param func
 * A pointer to the function.
 */
void comm_can_set_dw_uptime_func(void(*func)(uint8_t id, uint32_t uptime)) {
	m_dw_uptime_func = func;
}

/**
 * Get status message by index.
 *
 * @param index
 * Index in the array
 *
 * @return
 * The message or 0 for an invalid index.
 */
can_status_msg *comm_can_get_status_msg_index(int index) {
	if (index < CAN_STATUS_MSGS_TO_STORE) {
		return &stat_msgs[index];
	} else {
		return 0;
	}
}

/**
 * Pinga VESC-ID 0-253 på CAN-bussen, som VESC Tools "Scan CAN" gör, och
 * lista de som svarar. Hittar alla VESC, även de som inte skickar CAN-status.
 * Blockerar anroparen i drygt 100 ms.
 *
 * @param ids
 * Fylls med ID:n som svarade, i stigande ordning.
 *
 * @param max
 * Plats i ids.
 *
 * @return
 * Antal ID:n i ids.
 */
int comm_can_ping_vescs(uint8_t *ids, int max) {
	for (int i = 0;i < 32;i++) {
		pong_seen[i] = 0;
	}

	uint8_t sender = CAN_PING_SENDER_ID;
	for (int id = 0;id < 254;id++) {
		comm_can_transmit_eid(id | ((uint32_t)CAN_PACKET_PING << 8), &sender, 1);
	}

	// VESC svarar inom några ms; ge sista pingen marginal.
	chThdSleepMilliseconds(100);

	int n = 0;
	for (int id = 0;id < 254 && n < max;id++) {
		if (pong_seen[id >> 3] & (1 << (id & 7))) {
			ids[n++] = (uint8_t)id;
		}
	}
	return n;
}

/**
 * Get status message by id.
 *
 * @param id
 * Id of the controller that sent the status message.
 *
 * @return
 * The message or 0 for an invalid id.
 */
can_status_msg *comm_can_get_status_msg_id(int id) {
	for (int i = 0;i < CAN_STATUS_MSGS_TO_STORE;i++) {
		if (stat_msgs[i].id == id) {
			return &stat_msgs[i];
		}
	}

	return 0;
}

bool comm_can_addio_lim_sw(int sw) {
	return addio_lim_sw[sw];
}

float comm_can_ftr2_angle(void) {
	if (!ftr2_activated) {
		uint8_t packet[8] = {0};
		uint8_t ind = 8;

		comm_can_transmit_sid(ftr2_id, ftr2_frame, 8);
	}
	return can_ftr2_angle;
}

void comm_can_addio_set_valve(int valve, bool set){
	if (iDebug==77)
	{
	commands_printf("I comm_can_addio_set_valve. Valve: %i, set: %ui",valve, set);
	}
	uint8_t packet[2] = {0};
	int32_t ind = 2;

	packet[0] = 0x24;
	packet[1] = update_addio_outputs(valve, set);

	comm_can_transmit_sid(0x1F, packet, ind);
}

static uint8_t update_addio_outputs(int valve, bool set) {
	static uint8_t addio_board_outputs_bm = 0;
	if (valve > 7) return addio_board_outputs_bm;

	if (set) {
		addio_board_outputs_bm |= (1 << valve);
	} else {
		addio_board_outputs_bm &= ~(1 << valve);
	}

	return addio_board_outputs_bm;
}

static void prop_valve_status_sm(uint8_t node_id, uint8_t* data){
	uint8_t packet[8] = {0};
	uint8_t ind = 8;

	switch (data[0])
	{
	case PVG32_ERROR:
		packet[0] = 0x81; //Reset
		packet[1] = node_id; //node-ide
		comm_can_transmit_sid(0x00, packet, ind);
		// comm_can_transmit_sid(0x300 + node_id, packet, ind);
		break;
	case PVG32_INIT:
		packet[0] = PVG32_DISABLED;
		packet[2] = 0x01;
		comm_can_transmit_sid(0x300 + node_id, packet, ind);
		break;
	case PVG32_DISABLED:
		packet[0] = PVG32_HOLD;
		packet[2] = 0x01;
		comm_can_transmit_sid(0x300 + node_id, packet, ind);
		break;
	case PVG32_HOLD:
		packet[0] = PVG32_ACTIVE;
		packet[2] = 0x01;
		comm_can_transmit_sid(0x300 + node_id, packet, ind);
		break;
	case PVG32_ACTIVE:
		break;
	
	default:
		break;
	}
}

static void prop_valve_nmt_sm(uint8_t node_id, uint8_t state){
	uint8_t packet[2];
	uint8_t ind = 2;
	switch (state)
	{
	case PVG32_BOOTUP:
		break;
	case PVG32_STOPPED:
		pvg32_node_id = node_id;
		packet[0] = 0x01;
		packet[1] = node_id;

		comm_can_transmit_sid(0x00, packet, ind);
		break;
	case PVG32_PREOPERATIONAL:
		pvg32_node_id = node_id;
		packet[0] = 0x01;
		packet[1] = node_id;

		comm_can_transmit_sid(0x00, packet, ind);
		break;
	case PVG32_OPERATIONAL:
		pvg32_node_id = node_id;
		break;
	
	default:
		break;
	}
}

void comm_can_addio_set_valve_duty(float duty) {
	servo_output=duty;

	uint8_t packet[8] = {0};
	int32_t ind = 8;

    // Convert duty (-1.0 - 1.0) to Vpoc setpoint (-16384 to +16384)
    int16_t vpoc_value = (int16_t)((duty * 32768.0f) - 16384.0);

    // Construct RxPDO1 message
	packet[2] = (uint8_t)(vpoc_value & 0xFF);
	packet[3] = (uint8_t)((vpoc_value >> 8) & 0xFF);

    // Send RxPDO1 message (0x200 + NodeID)
	comm_can_transmit_sid(0x200 + pvg32_node_id, packet, ind);
}

float comm_can_io_board_adc_voltage(int ch) {
	if (ch < 0 || ch >= 8) {
		return 0.0;
	}
	return io_board_adc_voltages[ch];
}

float comm_can_io_board_as5047_angle(void) {
	return io_board_as5047_angle;
}

void comm_can_io_board_as5047_setangle(float angle) {
	io_board_as5047_angle=angle;
}


bool comm_can_io_board_lim_sw(int sw) {
	if (sw < 0 || sw >= 8) {
		return false;
	}
	return io_board_lim_sw[sw];
}

#ifndef SERVO_READ
ADC_CNT_t* comm_can_io_board_adc0_cnt(void) {
	return &io_board_adc0_cnt;
}
#endif


void comm_can_io_board_set_valve(int board, int valve, bool set) {
	uint8_t packet[8];
	int32_t ind = 0;
	packet[ind++] = valve;
	packet[ind++] = set;
	comm_can_transmit_sid((board & 0x0F) | CAN_MASK_IO_BOARD | (CAN_IO_PACKET_SET_VALVE << 4), packet, ind);
}

void comm_can_io_board_set_pwm_duty(int board, float duty) {
	uint8_t packet[8];
	int32_t ind = 0;
	buffer_append_float16(packet, duty, 1e3, &ind);
	comm_can_transmit_sid((board & 0x0F) | CAN_MASK_IO_BOARD | (CAN_IO_PACKET_SET_VALVE_PWM_DUTY << 4), packet, ind);
}

static void send_packet_wrapper(unsigned char *data, unsigned int len) {
	comm_can_send_buffer(vesc_id, data, len, false);
}

static void printf_wrapper(char *str) {
	commands_printf(str);
}


// CAN-kretsens felregister (ESR): TEC/REC, senaste felkod och bus-off.
// Används för diagnostik: TEC som växer och LEC=3 (ACK-fel) betyder att ingen
// nod på bussen kvitterar våra ramar (fel hastighet, kabel eller strömlös VESC).
uint32_t comm_can_get_esr(void) {
	return CANDx.can->ESR;
}
