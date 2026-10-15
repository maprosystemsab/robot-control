
/*
	Copyright 2016 - 2017 Benjamin Vedder	benjamin@vedder.se

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

#ifndef CONF_GENERAL_H_
#define CONF_GENERAL_H_

#include "datatypes.h"

#define MAIN_MODE_VEHICLE 				0

// Main mode
#ifndef MAIN_MODE
#define MAIN_MODE					MAIN_MODE_VEHICLE
#endif

// Mode macros
#define MAIN_MODE_IS_VEHICLE		(MAIN_MODE == MAIN_MODE_VEHICLE)

// Firmware version
#define FW_VERSION_MAJOR			30
#define FW_VERSION_MINOR			5	// 30.5 (2026-10-15): självcentrerande styrning med vinkelgivaren (styr_position.c), hjuldiameter RobAnt 0,46 m. 30.4 (2026-10-13): CMD_PING_VESC_CAN (141) pingar VESC på CAN som VESC Tool, CMD_TEST_SPIN_VESC (142) provsnurr, RobAnt läser batteriet från VESC. 30.3 (2026-09-30): som 30.2 + autopiloten sätter körriktningen. 30.2 (2026-09-29): Write-hängningen rättad, heartbeat i sekunder, stopp

// IO BOARD
// #define IO_BOARD

#ifndef IS_MACTRAC
#ifndef IS_ROBANT
#define DRANGEN_NY
#endif
#endif
//#define DRANGEN_NY
#define COMMUNICATION_TIMEOUT_MSEC 6000


#ifdef DRANGEN_NY
#define IS_DRANGEN
#define VESC_LEFT 94
#define VESC_RIGHT 125
#define VESC_STEERING 113 // 9
#define LOADING 8
#define WHEEL_SENSOR                1
#endif

// RobAnt 3 — VESC-ID:n bekräftade på riktig hårdvara 2026-09-22
// (vänster=28, höger=36, styrning=76). Eget block så att Drängen/Mactrac
// inte påverkas.
#ifdef IS_ROBANT
#define IS_DRANGEN
#define ROBANT_VIN_FROM_VESC	// batteriet läses från VESC, styrkortet kan matas med 12 V
#define VESC_LEFT 28
#define VESC_RIGHT 36
#define VESC_STEERING 76
#define LOADING 8
#define WHEEL_SENSOR                1
// Hastighetsgivare ZF GS1001 på TX-kontakten (PA2): pulser från en stålskiva med hål.
// PA2 får intern pull-up (givaren har öppen kollektor).
#define WHEELSPEED_DIAM             0.46	// m, rullande diameter: ytterdiameter ca 450 mm, omkrets ca 1500 mm (mk1, 2026-10-14); finjustera med en uppmätt sträcka
#define WHEELSPEED_CNTS_PER_REV     13.0	// 13 hål Ø20 mm på Ø140 mm hålbild, skiva Ø169 mm på hjulaxeln
#ifndef IS_ROVMCU	// MP101: hjulpulserna på PA3 med extern pull-up (R127), se pwm_esc.c
#define WHEELSPEED_PULLUP
// Vinkelgivare DIS QR30N-360 (0–5 V för 0–360°) på RX-kontakten (PA3) via
// spänningsdelare R1 = 12k (givare–S), R2 = 22k (S–GND). PA3 blir analog ingång
// i stället för servoutgång. Vinkeln = (U - CENTER) * DEG_PER_MV.
#define ANGLE_SENSOR_PA3
#define ANGLE_SENSOR_DIVIDER        ((12.0 + 22.0) / 22.0)
#define ANGLE_SENSOR_CENTER_MV      2500.0	// givarens spänning rakt fram — TODO: kalibrera
#define ANGLE_SENSOR_DEG_PER_MV     0.072	// 360° / 5000 mV; byt tecken om vinkeln går åt fel håll
#define ANGLE_SENSOR_MIN_MV         100.0	// under detta: magneten saknas/givarfel
#define ANGLE_SENSOR_MAX_DEG        30.0	// styrvinkel vid fullt utslag (vinkel_hoger/vinkel_vanster)
#endif	// IS_ROVMCU: vinkelgivaren på PA3 finns bara på det gamla kortet
#endif

#ifdef DRANGEN_GAMMAL
#define IS_DRANGEN
#define VESC_LEFT 28
#define VESC_RIGHT 36
#define VESC_STEERING 16
#endif

//#define IS_MACTRAC
#define UPDATE20250812

// MacTrac
// Steering Center: 210
// Valve: Low values: Turn right; high values: turn left
#ifdef IS_MACTRAC
#define HAS_HYDRAULIC_DRIVE			1
#define HAS_PWM_ESC                 1
#define WHEEL_SENSOR                1
#define SERVO_VESC_S1				178.0 // Left
#define SERVO_VESC_S2				240.0 // Right
#define USE_ADCONV_FOR_VIN
#define SERVO_VESC_HYDRAULIC
#define HYDRAULIC_HAS_SPEED_SENSOR
#define SERVO_VESC_ID				0
#define SERVO_VESC_INVERTED			0
//#define SERVO_VESC_DEADBAND_COMP    0.2
#define IS_F9_BOARD					1
#define ADDIO
#define CAN_ADDIO					1
#define SERVO_WRITE
#define SERVO_READ
#define MAX_RATIO                 2.0
#define MIN_RATIO                 0.5
#define MAX_RATIO_LOWSPEED        4.0
#define MIN_RATIO_LOWSPEED        0.25
#define LOWSPEED                  0.5
#endif


#ifdef IS_DRANGEN
//#define HAS_HYDRAULIC_DRIVE			1
#define SERVO_VESC_ID				0
#define VESC_ID				0
#define VOLTAGEFRACTION 1
#define SERVO_VESC_S1				180.7 // Left
#define SERVO_VESC_S2				237.7 // Right
#ifndef ROBANT_VIN_FROM_VESC
#define USE_ADCONV_FOR_VIN
#endif
#define SERVO_VESC_HYDRAULIC
#define HYDRAULIC_HAS_SPEED_SENSOR
#define SERVO_VESC_INVERTED			0
#define SERVO_VESC_DEADBAND_COMP    0.2
#define IS_F9_BOARD					1
#define IS_ALL_ELECTRIC             1
#define ANALOG_ANGLE
#define SERVO_WRITE                 1
#define SERVO_READ                  1
#define WHEEL_SENSOR                1
#define HAS_PWM_ESC                 1
#define STEERINGANGLE_MAX           25.0
//#define HAS_HYDRAULIC_DRIVE			1
//#define TACHOATCARD
#define MAX_RATIO                 999
#define MIN_RATIO                 0.001
#define MAX_RATIO_LOWSPEED        999
#define MIN_RATIO_LOWSPEED        0.001
#define LOWSPEED                  0.5
#endif

#ifdef CAN_ADDIO
#define FTR2_ANGLE                  1
#endif

// Differential steering
#ifndef VESC_LEFT
#define VESC_LEFT		0
#endif
#ifndef VESC_RIGHT
#define VESC_RIGHT	1
#endif

// VESC for steering
// ID of VESC for steering.
// -1: No steering VESC
#ifndef SERVO_VESC_ID
#define SERVO_VESC_ID				-1
#endif
/*
 * Angle should be increasing from S1 to S2 (possibly
 * passing 0). The steering mapping is done on top
 * of S1 and S2.
 */
#ifndef SERVO_VESC_S1
#define SERVO_VESC_S1				331.0
#endif
#ifndef SERVO_VESC_S2
#define SERVO_VESC_S2				30.0
#endif
#ifndef SERVO_VESC_P_GAIN
#define SERVO_VESC_P_GAIN			2.0
#endif
#ifndef SERVO_VESC_I_GAIN
#define SERVO_VESC_I_GAIN			1.0
#endif
#ifndef SERVO_VESC_D_GAIN
#define SERVO_VESC_D_GAIN			0.1
#endif
#ifndef SERVO_VESC_D_FILTER
#define SERVO_VESC_D_FILTER			0.05
#endif
#ifndef SERVO_VESC_INVERTED
#define SERVO_VESC_INVERTED			0
#endif
#ifndef SERVO_VESC_ANGLE_INVERTED
#define SERVO_VESC_ANGLE_INVERTED	0
#endif
#ifndef SERVO_VESC_DEADBAND_COMP
#define SERVO_VESC_DEADBAND_COMP	0.0 // Range 0 - 1
#endif

// Ublox settings
#ifndef UBLOX_EN
#define UBLOX_EN					1
#endif
#ifndef UBLOX_USE_PPS
#define UBLOX_USE_PPS				1
#endif

// External PPS signal for accurate time synchronization and delay compensation on PD4.
#ifndef GPS_EXT_PPS
#define GPS_EXT_PPS					0
#endif

// CAN settings
#define CAN_EN_DW					1

// Log configuration to enable. Choose one only.
//#define LOG_EN_CARREL
//#define LOG_EN_ITRANSIT

// General settings
#define ID_ALL						255
#define ID_VEHICLE_CLIENT				254 // Packet for vehicle client only
#ifndef VESC_ID
#define VESC_ID						ID_ALL // id, or ID_ALL for any VESC (not used in diff steering mode)
#endif
#define ID_MOTE						254 // If the packet is for the mote and not to be forwarded in mote mode

// vehicle parameters
#ifndef BOARD_YAW_ROT
#define BOARD_YAW_ROT				-90.0
#endif

// Servo settings
#define SERVO_OUT_RATE_HZ			50
#define SERVO_OUT_PULSE_MIN_US		1000
#define SERVO_OUT_PULSE_MAX_US		2000

// Autopilot settings
#define AP_ROUTE_SIZE				2000

// Board-dependent settings
#ifdef IS_ROVMCU
#define UBLOX_IS_F9P                 1
#define LED_RED_GPIO                 GPIOC
#define LED_RED_PIN                  11
#define LED_GREEN_GPIO               GPIOC
#define LED_GREEN_PIN                10
#define CAN1_RX_GPIO                 GPIOB
#define CAN1_RX_PIN                  8
#define CAN1_TX_GPIO                 GPIOB
#define CAN1_TX_PIN                  9
#define HAS_BMI160                   0
#define HAS_BMI270                   1
#define HAS_ID_SW                    0
#define PWR_5V_R1                    20000.0
#define PWR_5V_R2                    10000.0
// MP101 has no populated battery divider. Keep battery vin on the existing
// VESC/CAN telemetry path; PC1/IN11 measures the regulated 5 V rail only.
#ifdef USE_ADCONV_FOR_VIN
#undef USE_ADCONV_FOR_VIN
#endif
#elif IS_F9_BOARD
#define UBLOX_IS_F9P				1
#define LED_RED_GPIO				GPIOC
#define LED_RED_PIN					10
#define LED_GREEN_GPIO				GPIOC
#define LED_GREEN_PIN				11
#define CAN1_RX_GPIO				GPIOB
#define CAN1_RX_PIN					8
#define CAN1_TX_GPIO				GPIOB
#define CAN1_TX_PIN					9
#define HAS_BMI160					1
#define HAS_ID_SW					0
#define VIN_R1						39000.0
#define VIN_R2						2200.0
#else
//#define UBLOX_IS_F9P				0
#define LED_RED_GPIO				GPIOE
#define LED_RED_PIN					0
#define LED_GREEN_GPIO				GPIOE
#define LED_GREEN_PIN				1
#define CAN1_RX_GPIO				GPIOD
#define CAN1_RX_PIN					0
#define CAN1_TX_GPIO				GPIOD
#define CAN1_TX_PIN					1
#define HAS_BMI160					0
#define HAS_ID_SW					1
#define VIN_R1						10000.0
#define VIN_R2						1500.0
#endif

#ifndef HAS_BMI270
#define HAS_BMI270					0
#endif
#if HAS_BMI160 && HAS_BMI270
#error "Select only one IMU driver"
#endif

#ifndef M_PI
#define M_PI						D(3.14159265358979323846)
#endif
// Global variables
extern MAIN_CONFIG main_config;
extern int main_id;

// Functions
void conf_general_init(void);
void conf_general_get_default_main_config(MAIN_CONFIG *conf);
void conf_general_read_main_conf(MAIN_CONFIG *conf);
void conf_general_sanitize_main_config(MAIN_CONFIG *conf);
bool conf_general_store_main_config(MAIN_CONFIG *conf);

#endif /* CONF_GENERAL_H_ */
