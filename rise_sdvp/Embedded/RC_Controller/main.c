/*
	Copyright 2017 Benjamin Vedder	benjamin@vedder.se

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

#include "ch.h"
#include "hal.h"
#include "stm32f4xx_conf.h"

#include <stdio.h>
#include <math.h>
#include <string.h>
#include <stdlib.h>

#include "conf_general.h"
#include "comm_usb.h"
#include "commands.h"
#include "utils.h"
#include "mpu9150.h"
//#include "basic_rf.h"
#include "ext_cb.h"
#include "adconv.h"
#include "styr_position.h"
#include "led.h"
#include "packet.h"
#include "pos.h"
#include "comm_can.h"
#include "servo_simple.h"
#include "autopilot.h"
#include "timeout.h"
#include "log.h"
#include "ublox.h"
#include "srf10.h"
#include "pwm_esc.h"
//#include "mr_control.h"
#include "motor_sim.h"
#include "m8t_base.h"
#include "pos_uwb.h"
#include "fi.h"
#include "motor_control.h"
#include "hydraulic.h"
#include "timer.h"
#include "wheelspeed.h"
// #include "watchdog.h"
#include "actions.h"
//event_source_t emergency_event;

#define MS2ST(ms)   ((systime_t)((ms) * CH_CFG_ST_FREQUENCY / 1000))

/*
 * Timers used:
 * TIM6: Pos
 * TIM3: servo_simple and pwm_esc
 * TIM9: pwm_esc
 * TIM5: timer.c
 *
 * DMA/Stream	Device		Usage
 * 2, 4			ADC1		adconv
 * 1, 0			I2C1		I2C EXT (Overlap with SPI EXT)
 * 1, 6			I2C1		I2C EXT
 * 1, 2			I2C2		MPU9150
 * 1, 7			I2C2		MPU9150
 * 1, 3			SPI2		CC2520
 * 1, 4			SPI2		CC2520
 * 2, 0			SPI1		CC1120
 * 2, 3			SPI1		CC1120
 * 2, 1			UART6		UBLOX
 * 2, 6			UART6		UBLOX
 * 2, 5			UART1		UART EXT
 * 2, 7			UART1		UART EXT
 * 1, 0			SPI3		SPI EXT (Overlap with I2C EXT)
 * 1, 5			SPI3		SPI EXT
 *
 */

#ifdef IS_ROVMCU
/*
 * ROV_MCU (Upwis MP101_323): det som skiljer från F9-kortet, se ROVMCU.md.
 * - CAN-transceivrarna (TCAN1057A) har S-benet kopplat till STM32: hög = bara
 *   lyssna. Låg = normal drift, annars kan kortet inte skicka på CAN.
 * - DI1-4 (SERV_0-3) går via 74AXP1T45 med riktningsben: hög = STM32 -> kontakt.
 *   PB0/PB1/PA2 ut, PA3 in för hjulpulser (PD7 låg).
 * - GPS 2 (U7) har reset på PA15 (aktiv låg): håll den igång. GPS 1 sköts av ublox.c (PC9).
 * - PB3/PB4 är JTAG-ben efter reset; SWD (PA13/PA14) påverkas inte.
 */
static void rovmcu_board_init(void) {
	// CAN1 active; unused CAN2 stays silent.
	palClearPad(GPIOB, 15);
	palSetPadMode(GPIOB, 15, PAL_MODE_OUTPUT_PUSHPULL);	// SILENT_CAN
	palSetPad(GPIOB, 14);
	palSetPadMode(GPIOB, 14, PAL_MODE_OUTPUT_PUSHPULL);	// SILENT_CAN2

	// PB0/PB1/PA2 are PWM outputs; PA3 is the pulled-up wheel input.
	palSetPad(GPIOE, 4);	// DIR_U34: DI1 = SERV_0 (PB0)
	palSetPadMode(GPIOE, 4, PAL_MODE_OUTPUT_PUSHPULL);
	palSetPad(GPIOB, 3);	// DIR_U38: DI2 = SERV_1 (PB1)
	palSetPadMode(GPIOB, 3, PAL_MODE_OUTPUT_PUSHPULL);
	palSetPad(GPIOB, 4);	// DIR_U39: DI3 = SERV_2 (PA2)
	palSetPadMode(GPIOB, 4, PAL_MODE_OUTPUT_PUSHPULL);
	palClearPad(GPIOD, 7);	// DIR_U40: DI4 = SERV_3 (PA3), input
	palSetPadMode(GPIOD, 7, PAL_MODE_OUTPUT_PUSHPULL);

	// GPS 2 ur reset
	palSetPad(GPIOA, 15);	// FMU_CH77_F9P_RESET_N
	palSetPadMode(GPIOA, 15, PAL_MODE_OUTPUT_PUSHPULL);
}
#endif

int main(void) {
	halInit();
	chSysInit();

#ifdef IS_ROVMCU
	rovmcu_board_init();
#endif

    // Initialize event source
    //chEvtObjectInit(&emergency_event);

	timer_init();
	led_init();
	ext_cb_init();

	conf_general_init();
	actuators_init();
	adconv_init();
	styrpos_init();
	servo_simple_init();
	pos_init();
	pos_uwb_init();
	comm_can_init();
	autopilot_init();
//	watchdog_init();

	motor_control_init();
	timeout_init();
	log_init();
	motor_sim_init();
	commands_init();
#if HAS_HYDRAULIC_DRIVE
	hydraulic_init();
#endif
#if WHEEL_SENSOR
	hydraulic_init();
	tach_input_init();
	wheelspeed_init();
#endif
#if HAS_PWM_ESC
	pwm_esc_init();
	pwm_esc_set_all(0.5);
#endif
	comm_usb_init();
//	watchdog_init();

#if UBLOX_EN
	ublox_init();
#endif

	motor_sim_set_running(main_config.vehicle.simulate_motor);

	rtcm3_set_rx_callback_obs(pos_base_rtcm_obs, commands_get_rtcm3_state());

	fi_init();

	timeout_configure(timeout_heartbeat_ms(main_config.vehicle.heartbeat_maxtime), 40.0);
	log_set_rate(main_config.log_rate_hz);
	log_set_enabled(main_config.log_en);
	log_set_name(main_config.log_name);
	log_set_ext(main_config.log_mode_ext, main_config.log_uart_baud);

#if HAS_BMI270
	pos_start_imu();
#endif

	for(;;) {
		chThdSleepMilliseconds(10);
		packet_timerfunc();
#ifdef ANGLE_SENSOR_PA3
		adconv_update_angle();
		styrpos_update();
#endif
	}
}
