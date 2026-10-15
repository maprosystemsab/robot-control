/*
	Copyright 2026 Gunnar Larsson

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

#ifndef MOTOR_CONTROL_H
#define MOTOR_CONTROL_H

#include "datatypes.h"

// Motor control functions

void motor_set_speed(float speed);
void motor_set_throttle_and_steering(float throttle,float steering,float frontangle);
/*
void motor_set_rpm(int rpm);
void motor_set_duty_cycle(float duty);
void motor_set_current(float current);*/
int motor_get_direction(void);
float motor_rpm(float speed);
void motor_diff_rpms(float *rpm_r, float *rpm_l, float speed, float m_turn_rad_now);
void motor_set_mode(int _mode);
void motor_control_init(void);

#if !defined(HAS_HYDRAULIC_DRIVE) || !HAS_HYDRAULIC_DRIVE
void motor_diff_control(float throttle, float steering);
void motor_steering_control(float throttle, float steering,float  frontangle);
#endif
// Autopilot-specific functions
void motor_set_steering_autopilot(float steering_angle, float circle_radius);
void motor_set_speed_autopilot(float speed);
void motor_handle_route_end(void);
void motor_stop(void);
void motor_set_vesc_value(int id, float value,motor_control_mode mode);
bool motor_test_spin(uint8_t vesc_id);

// Function to get actuators by activity
ACTUATOR* motor_get_actuators_by_activity(uint16_t activity, int* count);

// RobAnt: drivsidorna var för sig och vändning på stället (bandvagnsvändning).
// Aktiviteterna skickas med CMD_RC_CONTROL_ADV. 12 är "Emergency Stop" i
// RControlStation och får inte användas här.
#define ACT_NR_FART            10  // fart, båda sidor (som förut)
#define ACT_NR_STYRNING        11
#define ACT_NR_FART_VANSTER    20  // aktuatorns aktivitet: vänster drivsida
#define ACT_NR_FART_HOGER      21  // aktuatorns aktivitet: höger drivsida
#define ACT_NR_VANDNING        22  // kommando −1…+1, + = medurs (vänster fram, höger bak)

// Tar hand om aktiviteterna ovan. Returnerar true om kommandot är hanterat,
// false om det ska gå den vanliga vägen (alla aktuatorer med samma aktivitet).
bool motor_rc_adv(uint8_t activity, float value);

#endif /* MOTOR_CONTROL_H */
