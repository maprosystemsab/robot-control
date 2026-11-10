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

#include "timeout.h"
#include "bldc_interface.h"
#include "autopilot.h"
#include "conf_general.h"
#include "pos.h"
#include "motor_control.h"
#include "commands.h"
#include <math.h>

// Private variables
static volatile systime_t m_timeout_msec;
static volatile systime_t m_last_update_time;
static volatile float m_timeout_brake_current;
static volatile bool m_has_timeout;

// Threads
static THD_WORKING_AREA(timeout_thread_wa, 1024);	// 30.25: anropar motor_rc_adv och printf
static THD_FUNCTION(timeout_thread, arg);

/**
 * Heartbeat i inställningarna (heartbeat_maxtime) är i sekunder, som i
 * RControlStation ("s"). Förut användes talet direkt som millisekunder, så
 * standardvärdet 3 gav 3 ms och stängde av autopiloten hela tiden. 0 = av.
 */
systime_t timeout_heartbeat_ms(float seconds) {
	if (!(seconds > 0.0)) {
		return 0;
	}
	return (systime_t)(seconds * 1000.0);
}

void timeout_init(void) {
//	m_timeout_msec = 2000;
	m_timeout_msec = timeout_heartbeat_ms(main_config.vehicle.heartbeat_maxtime);
	m_last_update_time = 0;
	m_timeout_brake_current = 0.0;
	m_has_timeout = false;

	chThdCreateStatic(timeout_thread_wa, sizeof(timeout_thread_wa), NORMALPRIO, timeout_thread, NULL);
}

void timeout_configure(systime_t timeout, float brake_current) {
	m_timeout_msec = timeout;
	m_timeout_brake_current = brake_current;
}

void timeout_reset(void) {
	m_last_update_time = chVTGetSystemTime();
}

bool timeout_has_timeout(void) {
	return m_has_timeout;
}

systime_t timeout_get_timeout_msec(void) {
	return m_timeout_msec;
}

float timeout_get_brake_current(void) {
	return m_timeout_brake_current;
}

static THD_FUNCTION(timeout_thread, arg) {
	(void)arg;

	chRegSetThreadName("Timeout");

	for(;;) {
		if (m_timeout_msec != 0 && chVTTimeElapsedSinceX(m_last_update_time) > MS2ST(m_timeout_msec)) {
#if MAIN_MODE == MAIN_MODE_vehicle
			if (!m_has_timeout) {
				// Firmware 30.25: när heartbeat löser ut nollas även manuell gas
				// och vändning, som vid nödstopp (förut stängdes bara autopiloten
				// av och resten lämnades åt VESC:ernas egen timeout).
				motor_rc_adv(ACT_NR_FART, 0.0);
				motor_rc_adv(ACT_NR_VANDNING, 0.0);
				motor_stop();
				commands_printf("Heartbeat: inga kommandon på %u ms, gas nollad och autopilot av",
						(unsigned int)m_timeout_msec);
			}
			autopilot_set_active(false);
			if (!main_config.vehicle.disable_motor && fabsf(pos_get_speed() * 3.6) > 2.0) {
				bldc_interface_set_current_brake(m_timeout_brake_current);
			}
#endif
			m_has_timeout = true;
		} else {
			m_has_timeout = false;
		}

		chThdSleepMilliseconds(10);
	}
}
