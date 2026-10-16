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
#include <math.h>
#include <stdlib.h>
#include "motor_control.h"
#include "bldc_interface.h"
#include "hydraulic.h"
#include "utils.h"
#include "ch.h"
#include "conf_general.h"
#include "autopilot.h"
#include "steering_control.h"
#include "commands.h"
#include "comm_can.h"
#include "servo_simple.h"
#include "terminal.h"
#include "styr_position.h"
#include "pos.h"
#include <stdio.h>
#include <string.h>
//#include "watchdog.h"

// Current motor direction: +1 (forward) or -1 (backward)
static volatile int current_motor_direction = 1;
static int mode;
static void vandning_init(void);
#ifdef ANGLE_SENSOR_PA3
// RobAnt: autopiloten styr bakbenen med vinkelgivaren och får differentialen
// (se längre ner, vid differentialen).
static bool ap_styr_bakben(float steering_angle_rad);
static void ap_fart(float speed);
static volatile bool m_ap_kan_styra = true;
#endif

// Safe float printing function for embedded systems
void print_float_safe(const char* name, float value) {
    int32_t int_bits;
    memcpy(&int_bits, &value, sizeof(float));
    
    // Print as integer and fractional parts
    int whole = (int)value;
    float fractional = fabs(value - whole);
    int millionths = (int)(fractional * 1000000.0f);
    
    commands_printf("%s: %d.%06d (raw: 0x%08X)", name, whole, millionths, int_bits);
}
extern float m_turn_rad_now;
int iCounterMotor=0;
extern int iDebug;

// Steering control configuration
static STEERING_CONTROL_CONFIG m_steering_config;
static bool m_steering_control_initialized = false;

void motor_set_mode(int _mode) {
	mode=_mode;
}

// Initialize motor control system
// Provsnurr från Robotstyrnings skanning: en VESC på fast duty en kort stund,
// sedan stopp. Styrkortet stoppar själv, även om länken tappas mitt i.
// Maskinen ska vara upphissad.
#define TEST_SPIN_DUTY		0.3
#define TEST_SPIN_MS		1000

static THD_WORKING_AREA(test_spin_thread_wa, 512);
static binary_semaphore_t m_test_spin_sem;
static volatile int m_test_spin_id = -1; // -1 = inget provsnurr pågår

static THD_FUNCTION(test_spin_thread, arg) {
	(void)arg;
	chRegSetThreadName("Provsnurr");

	for(;;) {
		chBSemWait(&m_test_spin_sem);
		int id = m_test_spin_id;

		// VESC släpper motorn om den inte hör något på en stund, så upprepa.
		systime_t start = chVTGetSystemTime();
		while (chVTTimeElapsedSinceX(start) < MS2ST(TEST_SPIN_MS)) {
			motor_set_vesc_value(id, TEST_SPIN_DUTY, MOTOR_CONTROL_DUTY);
			chThdSleepMilliseconds(50);
		}
		for (int i = 0;i < 3;i++) {
			motor_set_vesc_value(id, 0.0, MOTOR_CONTROL_DUTY);
			chThdSleepMilliseconds(20);
		}

		m_test_spin_id = -1;
	}
}

/**
 * Starta ett provsnurr på en VESC (TEST_SPIN_DUTY i TEST_SPIN_MS).
 *
 * @return
 * false om ett provsnurr redan pågår.
 */
bool motor_test_spin(uint8_t vesc_id) {
	if (m_test_spin_id >= 0) {
		return false;
	}
	m_test_spin_id = vesc_id;
	chBSemSignal(&m_test_spin_sem);
	return true;
}

void motor_control_init(void) {
	chBSemObjectInit(&m_test_spin_sem, true);
	chThdCreateStatic(test_spin_thread_wa, sizeof(test_spin_thread_wa), NORMALPRIO,
			test_spin_thread, NULL);

    // Configure steering control parameters
    m_steering_config.p_gain = 2.0;      // From servo_vesc.c
    m_steering_config.i_gain = 1.0;      // From servo_vesc.c
    m_steering_config.d_gain = 0.1;      // From servo_vesc.c
    m_steering_config.d_filter = 0.05;   // From servo_vesc.c
    m_steering_config.max_output = 1.0;  // Normalized output
    m_steering_config.deadband = main_config.vehicle.deadband;
    m_steering_config.max_i_term = 1.0;

    steering_control_init(&m_steering_config);
    m_steering_control_initialized = true;

    vandning_init();
}

// Common motor control functions

void motor_set_speed(float speed)
{
	if (iDebug==41)
		{
			iCounterMotor++;
			if (iCounterMotor == 50)
			{
				commands_printf("set_speed");
				commands_printf("Mode: %i",mode);
				commands_printf("Speed: %f",speed);
			iCounterMotor=0;
			}
		}
	/*
    // Check watchdog state - only allow motor operation if system is operational
    if (system_state != SYSTEM_STATE_OPERATIONAL) {
        // Force speed to 0 if watchdog indicates unsafe state
        speed = 0.0f;
    }
    */
    
    if (speed > 0.01f) {
        current_motor_direction = 1;
    } else if (speed < -0.01f) {
        current_motor_direction = -1;
    }
	if (iDebug==42)
	{
		iCounterMotor++;
		if (iCounterMotor == 50)
		{
			commands_printf("Mode: %i",RC_MODE_CURRENT);
			commands_printf("Speed: %f",speed);
			iCounterMotor=0;
		}
	}
#if HAS_HYDRAULIC_DRIVE
		// MacTrac uses this
		hydraulic_set_speed(speed);
#else
	#if HAS_DIFF_STEERING
			float rpm_r=0.0 ,rpm_l=0.0;
			void motor_diff_rpms(*rpm_r, *rpm_l, speed, m_turn_rad_now);
			comm_can_lock_vesc();
			comm_can_set_vesc_id(VESC_LEFT);
			bldc_interface_set_rpm((int)rpm_l);
			comm_can_set_vesc_id(VESC_RIGHT);
			bldc_interface_set_rpm((int)rpm_r);
			comm_can_unlock_vesc();
	#else
		float rpm = motor_rpm(speed);
		comm_can_lock_vesc();
		comm_can_set_vesc_id(VESC_LEFT);
		bldc_interface_set_rpm((int)(rpm));
		comm_can_set_vesc_id(VESC_RIGHT);
		bldc_interface_set_rpm((int)(rpm));
		comm_can_unlock_vesc();
	#endif
#endif
}

void motor_set_vesc_value(int id, float value,motor_control_mode mode)
{
	// Diagnostik, högst två gånger per sekund per VESC: CAN-felräknare och vad
	// VESC:en själv rapporterar i sin CAN-status (duty, rpm, ström, ålder).
	static systime_t last_print[256];
	if (id >= 0 && id < 256 && chVTTimeElapsedSinceX(last_print[id]) > MS2ST(500)) {
		last_print[id] = chVTGetSystemTimeX();
		uint32_t esr = comm_can_get_esr();
		can_status_msg *st = comm_can_get_status_msg_id(id);
		commands_printf("VESC out: id=%d mode=%d val=%d | CAN TEC=%d BOFF=%d | status: %s duty=%d rpm=%d I=%d age=%d",
				id, mode, (int)(value * 1000.0),
				(int)((esr >> 16) & 0xFF), (int)((esr >> 2) & 0x1),
				st ? "hord" : "aldrig hord",
				st ? (int)(st->duty * 1000.0) : 0,
				st ? (int)st->rpm : 0,
				st ? (int)(st->current * 10.0) : 0,
				st ? (int)ST2MS(chVTTimeElapsedSinceX(st->rx_time)) : -1);
	}

 	comm_can_lock_vesc();
	comm_can_set_vesc_id(id);
	switch (mode)
	{
		case MOTOR_CONTROL_DUTY:
			bldc_interface_set_duty_cycle(value);
			break;
		case MOTOR_CONTROL_CURRENT:
			bldc_interface_set_current(value);
			break;
		case MOTOR_CONTROL_RPM:
			bldc_interface_set_rpm(value);
			break;
	}
	comm_can_unlock_vesc();

}

/*
 *
	MOTOR_CONTROL_CURRENT_BRAKE,
	MOTOR_CONTROL_POS
 */

void motor_set_throttle_and_steering(float throttle,float steering,float frontangle)
{
	if (iDebug==42)
		{
			commands_printf("set_throttle_and_steering");
			commands_printf("Mode: %i",mode);
			commands_printf("Throttle: %f",throttle);
			commands_printf("Steering: %f",steering);
		}

    // Update direction - must be exactly +1 or -1
    if (throttle > 0.01f) {
        current_motor_direction = 1;
    } else if (throttle < -0.01f) {
        current_motor_direction = -1;
    }
    // If speed is near zero, keep previous direction

	#if HAS_HYDRAULIC_DRIVE
	if (iDebug==41)
	{
		iCounterMotor++;
		if (iCounterMotor == 50)
		{
			commands_printf("HHD");
			iCounterMotor=0;
		}
	}
		//KAN VARA HÄR
		hydraulic_set_throttle_raw(throttle);
		//hydraulic_set_speed(throttle / 10);
		steering = utils_map(steering, -1.0, 1.0,
			main_config.vehicle.steering_center + (main_config.vehicle.steering_range / 2.0),
			main_config.vehicle.steering_center - (main_config.vehicle.steering_range / 2.0));

		//Steering between 0-1
		servo_simple_set_pos_ramp(steering, true);
	#else
		#if HAS_DIFF_STEERING
			motor_diff_control(throttle, steering);
		#else
			motor_steering_control(throttle, steering,frontangle);
		#endif
	#endif
}


/*
 * 			case RC_MODE_PID: // In m/s
				#if HAS_DIFF_STEERING
					if (steering < 0.001) {
						autopilot_set_turn_rad(1e6);
					} else {
						autopilot_set_turn_rad(1.0 / steering);
					}
				#endif
				motor_set_speed(throttle);
				break;
 *
 */




#if !HAS_HYDRAULIC_DRIVE

void motor_diff_control(float throttle, float steering)
{
	comm_can_lock_vesc();
	switch (mode) {
		case RC_MODE_CURRENT:
			comm_can_set_vesc_id(VESC_LEFT);
			bldc_interface_set_current(throttle + throttle * steering);
			comm_can_set_vesc_id(VESC_RIGHT);
			bldc_interface_set_current(throttle - throttle * steering);
			break;
		case RC_MODE_DUTY:
			comm_can_set_vesc_id(VESC_LEFT);
			bldc_interface_set_duty_cycle(throttle + throttle * steering);
			comm_can_set_vesc_id(VESC_RIGHT);
			bldc_interface_set_duty_cycle(throttle - throttle * steering);
			break;
	}
	comm_can_unlock_vesc();
}

void motor_steering_control(float throttle, float steering,float  frontangle)
{
	comm_can_lock_vesc();
	switch (mode) {
		case RC_MODE_CURRENT:
			comm_can_set_vesc_id(VESC_LEFT);
			bldc_interface_set_current(throttle);
			comm_can_set_vesc_id(VESC_RIGHT);
			bldc_interface_set_current(throttle);
			break;
		case RC_MODE_DUTY:
			comm_can_set_vesc_id(VESC_LEFT);
			bldc_interface_set_duty_cycle(throttle);
			comm_can_set_vesc_id(VESC_RIGHT);
			bldc_interface_set_duty_cycle(throttle);
			break;
	}
	comm_can_unlock_vesc();
	float okdirections=SIGN(frontangle)==-SIGN(steering);
	bool nottooextreme;
	#ifdef STEERINGANGLE_MAX
		nottooextreme=fabs(frontangle)<STEERINGANGLE_MAX;
	#else
		nottooextreme=true;
	#endif

if (nottooextreme || okdirections)
	{
		comm_can_lock_vesc();
		comm_can_set_vesc_id(VESC_STEERING);
		bldc_interface_set_duty_cycle(steering*VOLTAGEFRACTION);
		comm_can_unlock_vesc();
	}
}
#endif

float motor_rpm(float speed)
{
	return speed / (main_config.vehicle.gear_ratio
			* (2.0 / main_config.vehicle.motor_poles) * (1.0 / 60.0)
			* main_config.vehicle.wheel_diam * M_PI);
}

void motor_diff_rpms(float *rpm_r, float *rpm_l, float speed, float m_turn_rad_now)
{
    float diff_speed_half = 0.0;
    if (fabsf(m_turn_rad_now) > 0.1) {
        diff_speed_half = speed * (main_config.vehicle.axis_distance / (2.0 * m_turn_rad_now));
    }

    *rpm_r = (speed + diff_speed_half) / (main_config.vehicle.gear_ratio
            * (2.0 / main_config.vehicle.motor_poles) * (1.0 / 60.0)
            * main_config.vehicle.wheel_diam * M_PI);

    *rpm_l = (speed - diff_speed_half) / (main_config.vehicle.gear_ratio
            * (2.0 / main_config.vehicle.motor_poles) * (1.0 / 60.0)
            * main_config.vehicle.wheel_diam * M_PI);
}

int motor_get_direction(void) {
    return current_motor_direction;
}

// Autopilot-specific functions
void motor_set_steering_autopilot(float steering_angle, float circle_radius) {
    #if HAS_DIFF_STEERING
        // For differential steering, use turn radius
        autopilot_set_turn_rad(circle_radius);
    #else
        // Scale maximum steering by speed
        float max_rad = main_config.vehicle.steering_max_angle_rad * autopilot_get_steering_scale();

        if (steering_angle >= max_rad) {
            steering_angle = max_rad;
        } else if (steering_angle <= -max_rad) {
            steering_angle = -max_rad;
        }

        #if HAS_HYDRAULIC_DRIVE
            // Use existing servo_vesc control for hydraulic systems
            float servo_pos = steering_angle
                    / ((2.0 * main_config.vehicle.steering_max_angle_rad)
                            / main_config.vehicle.steering_range)
                            + main_config.vehicle.steering_center;
            servo_simple_set_pos_ramp(servo_pos, true);
        #elif defined(ANGLE_SENSOR_PA3)
            // RobAnt: bakbenen till autopilotens vinkel med vinkelstyrningen.
            // Går det inte (ingen givare) stannar autopiloten i stället för att köra rakt.
            m_ap_kan_styra = ap_styr_bakben(steering_angle);
        #else
            #if HAS_DIFF_STEERING
                // Use generic steering control with frontangle feedback

                // Set target and get control output
                steering_control_set_target(steering_angle);
                float control_output = steering_control_update(m_turn_rad_now);

                // Apply control output to steering motor
                comm_can_lock_vesc();
                comm_can_set_vesc_id(VESC_STEERING);
                bldc_interface_set_duty_cycle(control_output * VOLTAGEFRACTION);
                comm_can_unlock_vesc();
            #endif
        #endif
    #endif
}

void motor_set_speed_autopilot(float speed) {
    // Hastighetsgivaren (wheelspeed.c) ser inte riktningen utan tar den härifrån.
    // Förut satte bara manuell körning riktningen, så efter en manuell backning
    // räknade kortet autopilotens framåtkörning som backning: positionen och kursen
    // mellan GPS-uppdateringarna drog åt fel håll (vingel, ikonen vred sig i svängar).
    if (speed > 0.01f) {
        current_motor_direction = 1;
    } else if (speed < -0.01f) {
        current_motor_direction = -1;
    }

    #if HAS_HYDRAULIC_DRIVE
        hydraulic_set_speed(speed);
    #else
        #if HAS_DIFF_STEERING
            float rpm_r, rpm_l;
            motor_diff_rpms(&rpm_r, &rpm_l, speed, m_turn_rad_now);
            comm_can_lock_vesc();
            comm_can_set_vesc_id(VESC_LEFT);
            bldc_interface_set_rpm((int)rpm_l);
            comm_can_set_vesc_id(VESC_RIGHT);
            bldc_interface_set_rpm((int)rpm_r);
            comm_can_unlock_vesc();
        #else
            #if defined(ANGLE_SENSOR_PA3)
                // RobAnt: duty med återkoppling från hastighetsgivaren (se ap_fart).
                // Varvtal (RPM) fungerade inte: hydraulpumpen behöver ~87 000 ERPM för
                // 3 km/h men fordonsinställningarna gav ~255.
                ap_fart(speed);
            #elif defined(VESC_LEFT)
                float rpm = motor_rpm(speed);
                comm_can_lock_vesc();
                comm_can_set_vesc_id(VESC_LEFT);
                bldc_interface_set_rpm((int)(rpm));
                comm_can_set_vesc_id(VESC_RIGHT);
                bldc_interface_set_rpm((int)(rpm));
                comm_can_unlock_vesc();
            #endif
        #endif
    #endif
}

/**
 * Stoppa direkt: gas i neutralt och (MacTrac) alla armar stilla. Förut låg gasen
 * kvar tills hydraulikens timeout (2 s) och armarna tills TIMEOUT_SECONDS_MOVE
 * (10 s) när autopiloten stängdes av eller rutten tog slut.
 * Maskiner med VESC:er stoppas av VESC:ernas egen timeout.
 */
void motor_stop(void) {
    #if HAS_HYDRAULIC_DRIVE
        hydraulic_set_speed(0.0);
        hydraulic_move(HYDRAULIC_POS_FRONT, HYDRAULIC_MOVE_STOP);
        hydraulic_move(HYDRAULIC_POS_REAR, HYDRAULIC_MOVE_STOP);
        hydraulic_move(HYDRAULIC_POS_EXTRA, HYDRAULIC_MOVE_STOP);
    #endif
}

void motor_handle_route_end(void) {
    motor_stop();

    // Center steering
    #if HAS_HYDRAULIC_DRIVE
        servo_simple_set_pos_ramp(main_config.vehicle.steering_center, false);
    #else
        #if HAS_DIFF_STEERING
            // Differential steering vehicles don't need steering centering
        #else
            #ifdef VESC_STEERING
                comm_can_lock_vesc();
                comm_can_set_vesc_id(VESC_STEERING);
                bldc_interface_set_duty_cycle(main_config.vehicle.steering_center * VOLTAGEFRACTION);
                comm_can_unlock_vesc();
            #endif
        #endif
    #endif
    
    // Apply brake
    if (!main_config.vehicle.disable_motor) {
        bldc_interface_set_current_brake(10.0);
    }
}

// ---------------------------------------------------------------------------
// RobAnt: drivsidor var för sig (aktivitet 20/21) och vändning på stället (22)
//
// Står drivmotorerna på aktivitet 10 som förut händer ingenting nytt här.
// Med VESC:erna på 20 (vänster) och 21 (höger):
//  - fart (10) går till båda sidor med samma värde (differential i sväng kommer
//    när vinkelgivaren är inkopplad),
//  - vändning (22) kör vänster = +v·max och höger = −v·max, med mjuk start och
//    stopp. Bara när fartkommandot är 0. Styrkommandon ignoreras under tiden.
//  - kommer inga vändningskommandon på VANDNING_TIMEOUT_MS rampas den ner.
//  - med vinkelgivare (styr_position.c) ställs bakbenen först raka (0°) och
//    hålls där under vändningen; sidorna startar först när de står raka.
//    Utan givare stoppas bara styrcylindern, som förut.
// ---------------------------------------------------------------------------
#define VANDNING_TIMEOUT_MS   300
#define VANDNING_FART_DOD     0.05f  // fartkommando under detta räknas som stillastående
#define VANDNING_RAKT_DEG     3.5f   // bakbenen räknas som raka inom så här många grader (över styrningens återstartsgräns 3°)
#define VANDNING_CENTRERA_MS  4000   // hinner de inte bli raka på den tiden: avbryt

extern float frontangle;

static float m_vand_max = 0.3f;      // största duty vid vändning
static float m_vand_ramp_s = 0.7f;   // tid från 0 till max
static volatile float m_vand_mal = 0.0f;   // −1…+1 från dosan
static volatile float m_vand_ut = 0.0f;    // utgående −max…+max efter rampen
static volatile bool m_vand_aktiv = false; // sidorna drivs av vändningen just nu
static volatile bool m_vand_centrerar = false; // väntar på att bakbenen ska bli raka
static volatile systime_t m_vand_start = 0;
static volatile systime_t m_vand_senast = 0;
static volatile float m_fart_senast = 0.0f;
static volatile systime_t m_fart_tid = 0;

static THD_WORKING_AREA(vandning_thread_wa, 512);

static bool har_aktivitet(uint16_t activity) {
	for (int i = 0; i < main_config.vehicle.actuators && i < 4; i++) {
		if (main_config.vehicle.actuator[i].activity == activity) {
			return true;
		}
	}
	return false;
}

static bool sidor_konfigurerade(void) {
	return har_aktivitet(ACT_NR_FART_VANSTER) || har_aktivitet(ACT_NR_FART_HOGER);
}

static void skicka_till(uint16_t activity, float value) {
	for (int i = 0; i < main_config.vehicle.actuators && i < 4; i++) {
		ACTUATOR *a = &main_config.vehicle.actuator[i];
		if (a->activity == activity && a->type == AT_VESCMOTOR) {
			motor_set_vesc_value(a->motorid, value, a->mode);
		}
	}
}

// Bakbenen till 0° med vinkelstyrningen. Falskt = går inte (ingen givare/kalibrering).
static bool centrera_bakben(void) {
	bool ok = false;
	for (int i = 0; i < main_config.vehicle.actuators && i < 4; i++) {
		ACTUATOR *a = &main_config.vehicle.actuator[i];
		if (a->activity == ACT_NR_STYRNING && a->type == AT_VESCMOTOR) {
			ok = styrpos_command(a->motorid, 0.0f) || ok;
		}
	}
	return ok;
}

static void skicka_sidor(float vanster, float hoger) {
	skicka_till(ACT_NR_FART_VANSTER, vanster);
	skicka_till(ACT_NR_FART_HOGER, hoger);
}

// ---------------------------------------------------------------------------
// RobAnt: mjuk gas på drivsidorna (firmware 30.14). Hydraulpumpen drog så mycket
// ström vid språng från stillastående (t.ex. 0 -> 0,77 duty) att VESC:en löste ut
// FAULT_CODE_ABS_OVER_CURRENT (mk1 2026-10-15, framför allt med RCS som skickar
// spaken utan ramp). Sidornas duty ändras högst m_ramp_upp/s bort från noll och
// m_ramp_ned/s mot noll. Kommer inga fartkommandon på FART_TIMEOUT_MS går målet
// till 0, så att VESC:ernas egen timeout inte sätts ur spel.
// ---------------------------------------------------------------------------
#define FART_TIMEOUT_MS 500
static float m_ramp_upp = 1.0f;		// duty/s: 0 -> 0,7 på 0,7 s
static float m_ramp_ned = 2.5f;		// duty/s mot noll (släppt gas, bromsning)
static volatile float m_fart_mal_v = 0.0f, m_fart_mal_h = 0.0f;
static volatile systime_t m_fart_mal_tid = 0;
static float m_fart_ut_v = 0.0f, m_fart_ut_h = 0.0f;
static bool m_fart_skickad_noll = true;

static float ramp_steg(float ut, float mal, float dt) {
	if (ut == mal) {
		return ut;
	}
	// Bort från noll (samma tecken och större): långsamt. Annars mot noll: fortare.
	bool upp = (mal > 0.0f && ut >= 0.0f && mal > ut) || (mal < 0.0f && ut <= 0.0f && mal < ut);
	float steg = (upp ? m_ramp_upp : m_ramp_ned) * dt;
	if (!upp && ((ut > 0.0f && mal < 0.0f) || (ut < 0.0f && mal > 0.0f))) {
		mal = 0.0f;	// byte av riktning: först ner till noll
	}
	if (ut < mal) {
		return (ut + steg > mal) ? mal : ut + steg;
	}
	return (ut - steg < mal) ? mal : ut - steg;
}

/** Ett steg av gasrampen (anropas var 20:e ms när ingen vändning pågår). */
static void fart_ramp(float dt) {
	float mv = m_fart_mal_v, mh = m_fart_mal_h;
	if (ST2MS(chVTTimeElapsedSinceX(m_fart_mal_tid)) > FART_TIMEOUT_MS) {
		mv = mh = 0.0f;
	}
	if (m_ramp_upp <= 0.0f) {	// rampen avslagen: direkt
		m_fart_ut_v = mv;
		m_fart_ut_h = mh;
	} else {
		m_fart_ut_v = ramp_steg(m_fart_ut_v, mv, dt);
		m_fart_ut_h = ramp_steg(m_fart_ut_h, mh, dt);
	}
	// Hastighetsgivaren ser inte riktningen utan tar den från current_motor_direction.
	// Manuell körning (dosan, CMD_RC_CONTROL_ADV) satte den aldrig, så backning
	// räknades som framåt: positionen gick åt fel håll och kursen vreds i svängar
	// (mk1 2026-10-16). Nu sätts den här av det som faktiskt går ut till sidorna,
	// för både dosan och autopiloten. Nära noll behålls senaste riktningen (rullar ut).
	float medel = 0.5f * (m_fart_ut_v + m_fart_ut_h);
	if (medel > 0.02f) {
		current_motor_direction = 1;
	} else if (medel < -0.02f) {
		current_motor_direction = -1;
	}

	bool noll = m_fart_ut_v == 0.0f && m_fart_ut_h == 0.0f;
	if (!noll || !m_fart_skickad_noll) {
		skicka_sidor(m_fart_ut_v, m_fart_ut_h);
		m_fart_skickad_noll = noll;
	}
}

static THD_FUNCTION(vandning_thread, arg) {
	(void)arg;
	chRegSetThreadName("Vandning");
	const float dt = 0.02f;

	for (;;) {
		chThdSleepMilliseconds(20);

		if (!m_vand_aktiv) {
			if (sidor_konfigurerade()) {
				fart_ramp(dt);
			}
			continue;
		}
		// Under vändningen styr den sidorna; gasrampen börjar om från noll efteråt.
		m_fart_ut_v = m_fart_ut_h = 0.0f;
		m_fart_mal_v = m_fart_mal_h = 0.0f;
		m_fart_skickad_noll = true;

		float mal = m_vand_mal;
		if (ST2MS(chVTTimeElapsedSinceX(m_vand_senast)) > VANDNING_TIMEOUT_MS) {
			mal = 0.0f;
			m_vand_mal = 0.0f;
		}

		// Håll bakbenen raka (förnyar målet så att vinkelstyrningen inte släpper).
		bool kan_centrera = centrera_bakben();
		if (m_vand_centrerar) {
			if (mal == 0.0f) {
				m_vand_centrerar = false;
				m_vand_aktiv = false;
				continue;
			}
			if (!kan_centrera || fabsf(frontangle) < VANDNING_RAKT_DEG) {
				m_vand_centrerar = false; // raka (eller ingen givare): börja vända
			} else if (ST2MS(chVTTimeElapsedSinceX(m_vand_start)) > VANDNING_CENTRERA_MS) {
				commands_printf("Vandning: bakbenen blev inte raka (%.1f grader), avbryter", (double)frontangle);
				m_vand_centrerar = false;
				m_vand_aktiv = false;
				continue;
			} else {
				continue; // sidorna står still medan bakbenen rätas ut
			}
		}

		float steg = m_vand_max / (m_vand_ramp_s > 0.05f ? m_vand_ramp_s : 0.05f) * dt;
		float ut = m_vand_ut;
		float mal_ut = mal * m_vand_max;
		if (ut < mal_ut) {
			ut = (ut + steg > mal_ut) ? mal_ut : ut + steg;
		} else if (ut > mal_ut) {
			ut = (ut - steg < mal_ut) ? mal_ut : ut - steg;
		}
		m_vand_ut = ut;
		// Vändning på stället: roboten flyttar sig inte, hastighetsgivaren ska inte
		// räknas som körd sträcka (0 = stilla, se wheelspeed.c).
		current_motor_direction = 0;

		skicka_sidor(ut, -ut);

		if (ut == 0.0f && mal == 0.0f) {
			m_vand_aktiv = false;
		}
	}
}

// ---------------------------------------------------------------------------
// RobAnt: elektronisk differential (firmware 30.8). I en sväng går innersidan
// en kortare båge än yttersidan. Bakbensstyrd: vridningscentrum ligger på
// framaxelns linje, R = hjulbas / tan(styrvinkel) från mitten. Yttersidan får
// spakens fart, innersidan (R − spårvidd/2) / (R + spårvidd/2) av den.
// m_diff_gain förstärker inbromsningen av innersidan (0 = ren geometri).
// Kräver vinkelgivaren (OK och kalibrerad); annars samma fart på båda sidor.
// ---------------------------------------------------------------------------
static bool m_diff_pa = true;
static float m_diff_hjulbas = 1.37f;   // m, fram- till bakaxel (RobAnt originalmått)
static float m_diff_sparvidd = 1.25f;  // m, mellan vänster och höger hjul
static float m_diff_gain = 0.0f;

/** Innersidans andel av yttersidans fart vid styrvinkeln (grader). 1 = rakt. */
static float diff_innerandel(float vinkel_deg) {
	float t = tanf(fabsf(vinkel_deg) * (float)M_PI / 180.0f);
	if (t < 1e-3f || m_diff_hjulbas <= 0.0f) {
		return 1.0f;
	}
	float r = m_diff_hjulbas / t;
	float h = m_diff_sparvidd / 2.0f;
	float andel = (r > h) ? (r - h) / (r + h) : 0.0f;
	andel = 1.0f - (1.0f - andel) * (1.0f + m_diff_gain);
	utils_truncate_number(&andel, 0.0f, 1.0f);
	return andel;
}

static void skicka_fart(float fart) {
	float vanster = fart, hoger = fart;
	if (m_diff_pa && styrpos_vinkel_giltig()) {
		float v = frontangle;	// + = höger: höger sida är innersidan
		float inre = fart * diff_innerandel(v);
		if (v > 0.0f) {
			hoger = inre;
		} else {
			vanster = inre;
		}
	}
	// Gasrampen (vändningstråden) skickar till VESC:erna.
	m_fart_mal_v = vanster;
	m_fart_mal_h = hoger;
	m_fart_mal_tid = chVTGetSystemTimeX();
}

#ifdef ANGLE_SENSOR_PA3
// Autopilotens vinkel räknas matematiskt (+ = vänster); vinkelgivaren har + = höger.
// Tecknet kan bytas med `diff aptecken` om autopiloten svänger åt fel håll.
static float m_ap_tecken = -1.0f;
static systime_t m_ap_varnat = 0;

static bool ap_styr_bakben(float steering_angle_rad) {
	float deg = steering_angle_rad * 180.0f / (float)M_PI * m_ap_tecken;
	bool ok = false;
	for (int i = 0; i < main_config.vehicle.actuators && i < 4; i++) {
		ACTUATOR *a = &main_config.vehicle.actuator[i];
		if (a->activity == ACT_NR_STYRNING && a->type == AT_VESCMOTOR) {
			ok = styrpos_target_deg(a->motorid, deg) || ok;
		}
	}
	if (!ok && chVTTimeElapsedSinceX(m_ap_varnat) > S2ST(5)) {
		m_ap_varnat = chVTGetSystemTimeX();
		commands_printf("Autopilot: kan inte styra (vinkelgivaren ej OK/kalibrerad, eller styrlage duty). Står still.");
	}
	return ok;
}

// Autopilotens fart på RobAnt: framkoppling (duty = fart / m_ap_ms_per_duty) plus
// en långsam I-del på skillnaden mot hastighetsgivaren. Går ut som manuell gas
// (skicka_fart): differential och gasramp med 20/21, annars direkt duty.
static float m_ap_ms_per_duty = 1.27f;	// mk1: 0,7 duty -> ca 3,2 km/h
static float m_ap_ki = 0.15f;			// duty per (m/s) och sekund
static float m_ap_i = 0.0f;
static systime_t m_ap_t = 0;

static void ap_fart(float speed) {
	float dt = ST2MS(chVTTimeElapsedSinceX(m_ap_t)) / 1000.0f;
	m_ap_t = chVTGetSystemTimeX();
	if (dt > 0.2f) {
		dt = 0.2f;
	}
	float duty = 0.0f;
	if (m_ap_kan_styra && fabsf(speed) >= 0.05f) {
		float fel = speed - pos_get_speed();
		m_ap_i += m_ap_ki * fel * dt;
		utils_truncate_number_abs(&m_ap_i, 0.3f);
		duty = speed / m_ap_ms_per_duty + m_ap_i;
		utils_truncate_number(&duty, -0.8f, 0.8f);
	} else {
		m_ap_i = 0.0f;	// står still, eller kan inte styra: stå still hellre än köra rakt
	}
	if (sidor_konfigurerade()) {
		skicka_fart(duty);
		return;
	}
	float v = duty, h = duty;
	if (m_diff_pa && styrpos_vinkel_giltig()) {
		float inre = duty * diff_innerandel(frontangle);
		if (frontangle > 0.0f) {
			h = inre;
		} else {
			v = inre;
		}
	}
	motor_set_vesc_value(VESC_LEFT, v, MOTOR_CONTROL_DUTY);
	motor_set_vesc_value(VESC_RIGHT, h, MOTOR_CONTROL_DUTY);
}
#endif

static void terminal_diff(int argc, const char **argv) {
	if (argc >= 2) {
		if (strcmp(argv[1], "pa") == 0) {
			m_diff_pa = true;
		} else if (strcmp(argv[1], "av") == 0) {
			m_diff_pa = false;
		} else if (argc >= 3) {
			float x = 0.0f;
			if (sscanf(argv[2], "%f", &x) == 1) {
				if (strcmp(argv[1], "hjulbas") == 0 && x > 0.2f && x < 5.0f) {
					m_diff_hjulbas = x;
				} else if (strcmp(argv[1], "sparvidd") == 0 && x > 0.2f && x < 5.0f) {
					m_diff_sparvidd = x;
				} else if (strcmp(argv[1], "gain") == 0 && x >= 0.0f && x <= 2.0f) {
					m_diff_gain = x;
				}
				else if (strcmp(argv[1], "ramp") == 0 && x >= 0.0f && x <= 10.0f) {
					m_ramp_upp = x;	// 0 = av
					if (argc >= 4) {
						float y = 0.0f;
						if (sscanf(argv[3], "%f", &y) == 1 && y > 0.0f && y <= 20.0f) {
							m_ramp_ned = y;
						}
					}
				}
#ifdef ANGLE_SENSOR_PA3
				else if (strcmp(argv[1], "aptecken") == 0 && x != 0.0f) {
					m_ap_tecken = x > 0.0f ? 1.0f : -1.0f;
				} else if (strcmp(argv[1], "apfart") == 0 && x > 0.1f && x < 20.0f) {
					m_ap_ms_per_duty = x;
				} else if (strcmp(argv[1], "api") == 0 && x >= 0.0f && x < 5.0f) {
					m_ap_ki = x;
				}
#endif
			}
		}
	}
	commands_printf("Differential: %s, hjulbas %.2f m, spårvidd %.2f m, gain %.2f. "
			"Innersidan vid 10° %.0f %%, 20° %.0f %%, 30° %.0f %%. Givare %s. Nollställs vid omstart.",
			m_diff_pa ? "PÅ" : "AV", (double)m_diff_hjulbas, (double)m_diff_sparvidd, (double)m_diff_gain,
			(double)(100.0f * diff_innerandel(10.0f)), (double)(100.0f * diff_innerandel(20.0f)),
			(double)(100.0f * diff_innerandel(30.0f)),
			styrpos_vinkel_giltig() ? "OK" : "saknas/okalibrerad: samma fart båda sidor");
	commands_printf("Gasramp: %.2f duty/s upp, %.2f duty/s ned (diff ramp UPP [NED], 0 = av). Nu V %.2f H %.2f.",
			(double)m_ramp_upp, (double)m_ramp_ned, (double)m_fart_ut_v, (double)m_fart_ut_h);
#ifdef ANGLE_SENSOR_PA3
	commands_printf("Autopilot: styr bakbenen med vinkelgivaren, tecken %+.0f (diff aptecken 1/-1), %s. "
			"Fart med duty: %.2f m/s per duty (diff apfart X), I %.2f (diff api X), mätt fart %.2f m/s.",
			(double)m_ap_tecken, m_ap_kan_styra ? "kan styra" : "KAN INTE STYRA (står still)",
			(double)m_ap_ms_per_duty, (double)m_ap_ki, (double)pos_get_speed());
#endif
}

bool motor_rc_adv(uint8_t activity, float value) {
	if (!sidor_konfigurerade()) {
		return false; // som förut
	}

	switch (activity) {
	case ACT_NR_FART:
		m_fart_senast = value;
		m_fart_tid = chVTGetSystemTimeX();
		if (m_vand_aktiv) {
			if (fabsf(value) < VANDNING_FART_DOD) {
				return true; // vändningen styr sidorna
			}
			// Gas under vändning: avbryt vändningen, vanlig körning tar över
			// (gasrampen börjar där vändningen var, inget språng).
			m_fart_ut_v = m_vand_ut;
			m_fart_ut_h = -m_vand_ut;
			m_fart_skickad_noll = false;
			m_vand_mal = 0.0f;
			m_vand_ut = 0.0f;
			m_vand_aktiv = false;
		}
		skicka_fart(value);
		return true;

	case ACT_NR_STYRNING:
		// Styr inte bakbenen medan roboten vänder på stället
		return m_vand_aktiv;

	case ACT_NR_VANDNING: {
		utils_truncate_number(&value, -1.0f, 1.0f);
		bool fart_nyss = ST2MS(chVTTimeElapsedSinceX(m_fart_tid)) < 500;
		if (value != 0.0f && fart_nyss && fabsf(m_fart_senast) >= VANDNING_FART_DOD) {
			commands_printf("Vandning: ignoreras, slapp gasen forst");
			return true;
		}
		if (value != 0.0f && !m_vand_aktiv) {
			if (!centrera_bakben()) {
				skicka_till(ACT_NR_STYRNING, 0.0f); // ingen givare: stoppa bara styrcylindern
			}
			m_vand_start = chVTGetSystemTimeX();
			m_vand_centrerar = true;
			m_vand_aktiv = true;
		}
		m_vand_mal = value;
		m_vand_senast = chVTGetSystemTimeX();
		return true;
	}

	default:
		return false;
	}
}

static void terminal_vandning(int argc, const char **argv) {
	if (argc >= 2) {
		float v = 0.0f;
		if (sscanf(argv[1], "%f", &v) == 1 && v >= 0.0f && v <= 0.6f) {
			m_vand_max = v;
		}
	}
	if (argc >= 3) {
		float r = 0.0f;
		if (sscanf(argv[2], "%f", &r) == 1 && r >= 0.0f && r <= 5.0f) {
			m_vand_ramp_s = r;
		}
	}
	commands_printf("Vandning: max %.2f duty, ramp %.2f s, sidor %s, aktiv %d, ut %.2f",
			(double)m_vand_max, (double)m_vand_ramp_s,
			sidor_konfigurerade() ? "20/21 inst." : "EJ inst. (anv. 10)",
			m_vand_aktiv, (double)m_vand_ut);
}

static void vandning_init(void) {
	chThdCreateStatic(vandning_thread_wa, sizeof(vandning_thread_wa),
			NORMALPRIO, vandning_thread, NULL);
	terminal_register_command_callback(
			"vandning",
			"Vandning pa stallet (RobAnt). Utan argument: visa lage.\n"
			"  max  - storsta duty, 0-0.6 (standard 0.3)\n"
			"  ramp - sekunder fran 0 till max (standard 0.7)",
			"[max] [ramp]",
			terminal_vandning);
	terminal_register_command_callback(
			"diff",
			"Elektronisk differential (RobAnt): innersidan saktar in i svang.\n"
			"  pa|av, hjulbas X, sparvidd X (meter), gain X (0 = ren geometri),\n"
			"  aptecken 1|-1 (autopilotens svangriktning), ramp UPP [NED] (duty/s, 0 = av)",
			"[pa|av|hjulbas X|sparvidd X|gain X|aptecken 1|-1|ramp UPP [NED]]",
			terminal_diff);
}

// Function to get actuators by activity
ACTUATOR* motor_get_actuators_by_activity(uint16_t activity, int* count) {
    // Use the live in-RAM config (the same one CMD_GET_MAIN_CONFIG reports back to the
    // GUI) instead of re-reading EEPROM on every single RC command. Re-reading from
    // EEPROM here meant that if a CMD_SET_MAIN_CONFIG write ever only partially
    // persisted to flash (the store loop can bail out early on a failed
    // EE_WriteVariable, silently, with no error reported anywhere), RAM and EEPROM
    // could disagree: the GUI's "Read" would show the just-written actuator activity
    // correctly (it reads RAM), while this function would still see the stale EEPROM
    // copy and never match any actuator — so the joystick would send the right
    // activity but no VESC would ever respond.

    // Initialize count to 0
    *count = 0;

    // Loop through all actuators in the configuration
    for (int i = 0; i < main_config.vehicle.actuators && i < 4; i++) {
        if (main_config.vehicle.actuator[i].activity == activity) {
            (*count)++;
        }
    }

    // If no actuators found, return NULL
    if (*count == 0) {
        return NULL;
    }

    // Allocate memory for the result (this should be freed by the caller)
    ACTUATOR* result = (ACTUATOR*)malloc(*count * sizeof(ACTUATOR));
    if (result == NULL) {
        *count = 0;
        return NULL;
    }

    // Copy matching actuators to the result array
    int result_index = 0;
    for (int i = 0; i < main_config.vehicle.actuators && i < 4; i++) {
        if (main_config.vehicle.actuator[i].activity == activity) {
            result[result_index] = main_config.vehicle.actuator[i];
            result_index++;
        }
    }

    return result;
}
