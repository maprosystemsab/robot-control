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

#include "conf_general.h"
#include "ch.h"
#include "hal.h"
#include "stm32f4xx_conf.h"
#include "eeprom.h"
#include "utils.h"
#include <string.h>
#include <math.h>
#include <stdio.h>
#include "terminal.h"
#include "commands.h"

// Settings
#define EEPROM_BASE_MAINCONF		1000
// Egna variabler utanför MAIN_CONFIG (RCS Write rör dem inte).
#define EEPROM_AUTOCENTRERING		2000

// Global variables
MAIN_CONFIG main_config;
int main_id = 0;
uint16_t VirtAddVarTab[NB_OF_VAR];

// Private functions
static void terminal_cmd_set_id(int argc, const char **argv);
static void terminal_cmd_set_id_quiet(int argc, const char **argv);

void conf_general_init(void) {
#if HAS_ID_SW
	palSetPadMode(GPIOE, 8, PAL_MODE_INPUT_PULLUP);
	palSetPadMode(GPIOE, 9, PAL_MODE_INPUT_PULLUP);
	palSetPadMode(GPIOE, 10, PAL_MODE_INPUT_PULLUP);
	palSetPadMode(GPIOE, 11, PAL_MODE_INPUT_PULLUP);
	palSetPadMode(GPIOE, 12, PAL_MODE_INPUT_PULLUP);
	palSetPadMode(GPIOE, 13, PAL_MODE_INPUT_PULLUP);
	palSetPadMode(GPIOE, 14, PAL_MODE_INPUT_PULLUP);
	palSetPadMode(GPIOE, 15, PAL_MODE_INPUT_PULLUP);

	chThdSleepMilliseconds(10);

	// Read address from switches
	main_id = (~(palReadPort(GPIOE) >> 8)) & 0x0F;
#else
	main_id = 0;
#endif

	memset(VirtAddVarTab, 0, sizeof(VirtAddVarTab));

	for (unsigned int i = 0;i < (sizeof(MAIN_CONFIG) / 2);i++) {
		VirtAddVarTab[i] = EEPROM_BASE_MAINCONF + i;
	}
	// Tabellen har plats för sizeof(MAIN_CONFIG) poster men MAIN_CONFIG använder
	// bara hälften. Måste stå i tabellen för att följa med när EEPROM-sidan byts.
	VirtAddVarTab[sizeof(MAIN_CONFIG) / 2] = EEPROM_AUTOCENTRERING;

	FLASH_Unlock();
	FLASH_ClearFlag(FLASH_FLAG_OPERR | FLASH_FLAG_WRPERR | FLASH_FLAG_PGAERR |
			FLASH_FLAG_PGPERR | FLASH_FLAG_PGSERR);
	EE_Init();

	conf_general_read_main_conf(&main_config);

	terminal_register_command_callback(
			"set_id",
			"Set the ID of the board.",
			"[id]",
			terminal_cmd_set_id);
	terminal_register_command_callback(
			"set_id_quiet",
			"Set the ID of the board, and print no reply.",
			"[id]",
			terminal_cmd_set_id_quiet);
}

/**
 * Autocentrering (vinkelstyrning med vinkelgivaren) på/av, sparat i styrkortet.
 * Aldrig sparat = på.
 */
bool conf_general_autocentrering(void) {
	uint16_t var;
	if (EE_ReadVariable(EEPROM_AUTOCENTRERING, &var) == 0) {
		return var != 0;
	}
	return true;
}

bool conf_general_store_autocentrering(bool on) {
	FLASH_ClearFlag(FLASH_FLAG_OPERR | FLASH_FLAG_WRPERR | FLASH_FLAG_PGAERR |
			FLASH_FLAG_PGPERR | FLASH_FLAG_PGSERR);
	utils_sys_lock_cnt();
	bool ok = EE_WriteVariable(EEPROM_AUTOCENTRERING, on ? 1 : 0) == FLASH_COMPLETE;
	utils_sys_unlock_cnt();
	return ok;
}

/**
 * Load the compiled default app_configuration.
 *
 * @param conf
 * A pointer to store the default configuration to.
 */
void conf_general_get_default_main_config(MAIN_CONFIG *conf) {
	// Default settings
	conf->mag_use = false;
	conf->mag_comp = true;
	conf->yaw_mag_gain = 1.2;

	conf->mag_cal_cx = 0.0;
	conf->mag_cal_cy = 0.0;
	conf->mag_cal_cz = 0.0;
	conf->mag_cal_xx = 1.0;
	conf->mag_cal_xy = 0.0;
	conf->mag_cal_xz = 0.0;
	conf->mag_cal_yx = 0.0;
	conf->mag_cal_yy = 1.0;
	conf->mag_cal_yz = 0.0;
	conf->mag_cal_zx = 0.0;
	conf->mag_cal_zy = 0.0;
	conf->mag_cal_zz = 1.0;

	conf->gps_ant_x = 0.0;
	conf->gps_ant_y = 0.0;
	conf->gps_comp = true;
	conf->gps_req_rtk = true;
	conf->gps_use_rtcm_base_as_enu_ref = true;
	conf->gps_corr_gain_stat = 0.05;
	conf->gps_corr_gain_dyn = 0.05;
	conf->gps_corr_gain_yaw = 1.0;
	conf->gps_send_nmea = true;
	conf->gps_use_ubx_info = true;
	conf->gps_ubx_max_acc = 0.12;

	conf->uwb_max_corr = 0.1;

	conf->ap_repeat_routes = true;
	conf->ap_base_rad = 0.8;
	conf->ap_rad_time_ahead = 0.8;
	conf->ap_mode_time = false;
	conf->ap_max_speed = 30.0 / 3.6;
	conf->ap_time_add_repeat_ms = 60 * 1000;

	conf->log_rate_hz = 50;
	conf->log_en = false;
	strcpy(conf->log_name, "New Log");
	conf->log_mode_ext = 0;
	conf->log_uart_baud = 115200;

	// Default vehicle settings
	conf->vehicle.yaw_use_odometry = false;
	conf->vehicle.yaw_imu_gain = 0.5;
	conf->vehicle.disable_motor = false;
	conf->vehicle.simulate_motor = false;
	conf->vehicle.clamp_imu_yaw_stationary = true;
	conf->vehicle.use_uwb_pos = false;

	conf->vehicle.gear_ratio = (1.0 / 3.0) * (21.0 / 37.0);
	conf->vehicle.wheel_diam = 0.11;
	conf->vehicle.motor_poles = 4.0;

	conf->vehicle.steering_max_angle_rad = 0.42041;
	conf->vehicle.steering_center = 0.5;
	conf->vehicle.steering_range = 0.58;
	conf->vehicle.steering_ramp_time = 0.0;
	conf->vehicle.axis_distance = 0.475;

	conf->vehicle.vesc_p_gain = 6.0;
	conf->vehicle.vesc_i_gain = 0.0;
	conf->vehicle.vesc_d_gain = 0.0;

	conf->vehicle.sensorinterval = 160;
	conf->vehicle.degreeinterval = 68;
	conf->vehicle.sensorcentre = 580;
	conf->vehicle.deadband = 0.05;
	conf->vehicle.heartbeat_maxtime = 3.0;

	conf->vehicle.actuators = 4;
	conf->vehicle.actuator[0].type = 0; // AT_VESCMOTOR
	conf->vehicle.actuator[0].motorid = 94;
	conf->vehicle.actuator[0].activity = 1; // ACT_FORWARD
	conf->vehicle.actuator[0].mode = 0; // MOTOR_CONTROL_DUTY

	conf->vehicle.actuator[1].type = 0; // AT_VESCMOTOR
	conf->vehicle.actuator[1].motorid = 125;
	conf->vehicle.actuator[1].activity = 1; // ACT_FORWARD
	conf->vehicle.actuator[1].mode = 0; // MOTOR_CONTROL_DUTY

	conf->vehicle.actuator[2].type = 0; // AT_VESCMOTOR
	conf->vehicle.actuator[2].motorid = 113;
	conf->vehicle.actuator[2].activity = 2; // ACT_FORWARD
	conf->vehicle.actuator[2].mode = 0; // MOTOR_CONTROL_DUTY

	conf->vehicle.actuator[3].type = 0; // AT_VESCMOTOR
	conf->vehicle.actuator[3].motorid = 8;
	conf->vehicle.actuator[3].activity = 3; // ACT_FORWARD
	conf->vehicle.actuator[3].mode = 0; // MOTOR_CONTROL_DUTY

	// Sensor configurations
	conf->vehicle.sensors = 3;
	conf->vehicle.sensor[0].type = 1; // ST_POSITION
	conf->vehicle.sensor[0].sensorid = 0;
	conf->vehicle.sensor[0].activity = 1; // SENS_STEERING
	conf->vehicle.sensor[0].reserved = 0;

	conf->vehicle.sensor[1].type = 1; // ST_POSITION
	conf->vehicle.sensor[1].sensorid = 1;
	conf->vehicle.sensor[1].activity = 1; // SENS_STEERING
	conf->vehicle.sensor[1].reserved = 0;

	conf->vehicle.sensor[2].type = 0; // ST_VOLTAGE
	conf->vehicle.sensor[2].sensorid = 2;
	conf->vehicle.sensor[2].activity = 0; // SENS_FORWARD
	conf->vehicle.sensor[2].reserved = 0;

	// State control configurations
	conf->vehicle.state_controls = 1;
	conf->vehicle.control[0].actuator_activity = 2; // ACT_LIFT
	conf->vehicle.control[0].sensor_activity = 2; // SENS_LIFT
	conf->vehicle.control[0].control_type = 1; // CT_POSITION
	conf->vehicle.control[0].target_value = 0.0f;
	conf->vehicle.control[0].kp = 1.0f;
	conf->vehicle.control[0].ki = 0.1f;
	conf->vehicle.control[0].kd = 0.05f;
	conf->vehicle.control[0].min_output = -0.5f;
	conf->vehicle.control[0].max_output = 0.5f;
	conf->vehicle.control[0].enabled = false;

	// Initialize other state controls to safe defaults
	for (int i = 1; i < 4; i++) {
		conf->vehicle.control[i].actuator_activity = 0; // ACT_FORWARD
		conf->vehicle.control[i].sensor_activity = 0; // SENS_FORWARD
		conf->vehicle.control[i].control_type = 0; // CT_OPEN_LOOP
		conf->vehicle.control[i].target_value = 0.0f;
		conf->vehicle.control[i].kp = 0.0f;
		conf->vehicle.control[i].ki = 0.0f;
		conf->vehicle.control[i].kd = 0.0f;
		conf->vehicle.control[i].min_output = -1.0f;
		conf->vehicle.control[i].max_output = 1.0f;
		conf->vehicle.control[i].enabled = false;
	}


	/*100
	// Custom parameters based on ID
	switch (main_id) {
	case 1:
		conf->vehicle.steering_center = 0.5;
		conf->gps_ant_x = 0.42;
		break;

	default:
		break;
	}
	*/

	// Only the SLU testbot for now
#if HAS_DIFF_STEERING
	conf->vehicle.gear_ratio = 1.0;
	conf->vehicle.axis_distance = 0.5;
//	conf->vehicle.wheel_diam = 0.3;
	conf->vehicle.motor_poles = 22.0;
//	conf->gps_ant_x = 0.5;
#endif



#ifdef IS_DRANGEN
	conf->vehicle.steering_center = 0.0;
	conf->vehicle.steering_range = -2.0;
//	conf->vehicle.axis_distance = 1.0;
		//conf->vehicle.steering_center = 0;
//		conf->vehicle.steering_range = 0;
	conf->vehicle.axis_distance = 1.45;
	conf->vehicle.steering_max_angle_rad = atanf(conf->vehicle.axis_distance / 1.5);
	conf->vehicle.steering_max_angle_rad =
	conf->vehicle.wheel_diam = 0.47;
//	conf->vehicle.wheel_diam = 0.8;
	conf->gps_ant_x = 0; // TODO: IMU_ROT_180 sign?
	conf->gps_ant_y = 0.25;

	// Mellan hjul fram back 145 cm
	// Mellan hjul höger vänster 122 cm

#endif

#ifdef IS_MACTRAC
	conf->vehicle.steering_center = 0.5;
	conf->vehicle.steering_range = -1.0;
//	conf->vehicle.steering_range = 1.0;
	conf->vehicle.axis_distance = 1.7;
	conf->vehicle.wheel_diam = 0.66;
	float turn_radius=6.0;
	conf->vehicle.steering_max_angle_rad = atanf(conf->vehicle.axis_distance / turn_radius);
	conf->vehicle.steering_max_angle_rad = 0.99;
	conf->gps_corr_gain_yaw = 2.0;
	conf->ap_base_rad = 4.0;
	conf->ap_repeat_routes = false;

	conf->vehicle.vesc_p_gain = 6.0; // 3.0
	conf->vehicle.vesc_i_gain = 1.0; // 0.5
	conf->vehicle.vesc_d_gain = 0.5; // 0.0
	conf->vehicle.yaw_use_odometry = true;
//	conf->vehicle.TURN RADIUS              UPPDATERA HÄR

	conf->gps_ant_x = 1.25;
	conf->gps_ant_y = 0.3;

	conf->log_en = true;
	conf->log_mode_ext = LOG_EXT_ETHERNET;
	conf->log_rate_hz = 10;
#endif
}

/**
 * Read MAIN_CONFIG from EEPROM. If this fails, default values will be used.
 *
 * @param conf
 * A pointer to a MAIN_CONFIG struct to write the configuration to.
 */
void conf_general_read_main_conf(MAIN_CONFIG *conf) {
	bool is_ok = true;
	uint8_t *conf_addr = (uint8_t*)conf;
	uint16_t var;

	for (unsigned int i = 0;i < (sizeof(MAIN_CONFIG) / 2);i++) {
		if (EE_ReadVariable(EEPROM_BASE_MAINCONF + i, &var) == 0) {
			conf_addr[2 * i] = (var >> 8) & 0xFF;
			conf_addr[2 * i + 1] = var & 0xFF;
		} else {
			is_ok = false;
			break;
		}
	}

	// Set the default configuration
	if (!is_ok) {
		conf_general_get_default_main_config(conf);
	}

	conf_general_sanitize_main_config(conf);
}

/**
 * Rimlighetskontroll av antalen aktuatorer/sensorer/reglerloopar (max 4 st var).
 * Skräp här (t.ex. från en Write där kortet läste förbi paketets slut) fick
 * reglerlooparna att indexera utanför minnet och kortet att hänga.
 */
void conf_general_sanitize_main_config(MAIN_CONFIG *conf) {
	if (conf->vehicle.actuators > 4) {
		conf->vehicle.actuators = 0;
	}

	if (conf->vehicle.sensors > 4) {
		conf->vehicle.sensors = 0;
		memset(conf->vehicle.sensor, 0, sizeof(conf->vehicle.sensor));
	}

	if (conf->vehicle.state_controls > 4) {
		conf->vehicle.state_controls = 0;
		memset(conf->vehicle.control, 0, sizeof(conf->vehicle.control));
	}

	for (int i = 0;i < 4;i++) {
		// bool i minnet kan innehålla vad som helst efter en trasig Write
		uint8_t *en = (uint8_t*)&conf->vehicle.control[i].enabled;
		if (*en > 1 || conf->vehicle.control[i].control_type > CT_TEMPERATURE) {
			*en = 0;
		}
	}
}

/**
 * Write MAIN_CONFIG to EEPROM.
 *
 * @param conf
 * A pointer to the configuration that should be stored.
 */
bool conf_general_store_main_config(MAIN_CONFIG *conf) {
//	RCC_APB1PeriphClockCmd(RCC_APB1Periph_WWDG, DISABLE);

	bool is_ok = true;
	uint8_t *conf_addr = (uint8_t*)conf;
	uint16_t var;

	FLASH_ClearFlag(FLASH_FLAG_OPERR | FLASH_FLAG_WRPERR | FLASH_FLAG_PGAERR |
			FLASH_FLAG_PGPERR | FLASH_FLAG_PGSERR);

	// Låset tas per variabel istället för runt hela loopen. MAIN_CONFIG är stor
	// (100+ variabler), och med ett enda lås runt hela loopen stod USB:ns avbrott
	// (och därmed all seriekommunikation) stilla i flera hundra ms i ett svep,
	// vilket fick Car_Client:s skrivning att slå i sin timeout ("Write timeout on
	// serial port"). Nu släpps låset mellan varje variabel så USB hinner andas.
	for (unsigned int i = 0;i < (sizeof(MAIN_CONFIG) / 2);i++) {
		var = (conf_addr[2 * i] << 8) & 0xFF00;
		var |= conf_addr[2 * i + 1] & 0xFF;

		utils_sys_lock_cnt();
		bool write_ok = (EE_WriteVariable(EEPROM_BASE_MAINCONF + i, var) == FLASH_COMPLETE);
		utils_sys_unlock_cnt();

		if (!write_ok) {
			is_ok = false;
			break;
		}
	}

//	RCC_APB1PeriphClockCmd(RCC_APB1Periph_WWDG, ENABLE);

	return is_ok;
}

static void terminal_cmd_set_id(int argc, const char **argv) {
	if (argc == 2) {
		int set = -1;
		sscanf(argv[1], "%d", &set);

		if (set < 0 || set > 254) {
			commands_printf("Invalid argument\n");
		} else {
			main_id = set;
			commands_printf("ID is now %d\n", main_id);
		}
	} else {
		commands_printf("Wrong number of arguments\n");
	}
}

static void terminal_cmd_set_id_quiet(int argc, const char **argv) {
	if (argc == 2) {
		int set = -1;
		sscanf(argv[1], "%d", &set);

		if (set < 0 || set > 254) {
			// Wrong id
		} else {
			main_id = set;
		}
	}
}
