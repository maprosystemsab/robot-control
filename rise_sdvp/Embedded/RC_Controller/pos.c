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

#include <math.h>
#include <string.h>
#include <stdio.h>
#include <stdlib.h>
#include "ch.h"
#include "hal.h"
#include "ahrs.h"
#include "stm32f4xx_conf.h"
#include "led.h"
#include "mpu9150.h"
#include "bldc_interface.h"
#include "utils.h"
#include "servo_simple.h"
#include "commands.h"
#include "ublox.h"
//#include "mr_control.h"
#include "srf10.h"
#include "terminal.h"
#include "comm_usb.h"
#include "pos_uwb.h"
#include "comm_can.h"
#include "hydraulic.h"
#include "pos.h"
#include "bmi160_wrapper.h"
#include "bmi270_wrapper.h"

// Defines
#define ITERATION_TIMER_FREQ			50000
#define POS_HISTORY_LEN					100

#define UNREMOVE_REMOVEDBYGUNNAR20250317

// Private variables
static ATTITUDE_INFO m_att;
static POS_STATE m_pos;
static GPS_STATE m_gps;
static bool m_attitude_init_done;
static float m_accel[3];
static float m_gyro[3];
static float m_mag[3];
static float m_mag_raw[3];
static mc_values m_mc_val;
static float m_imu_yaw_offset;
static mutex_t m_mutex_pos;
static mutex_t m_mutex_gps;
static int32_t m_ms_today;
static bool m_ubx_pos_valid;
static int32_t m_nma_last_time;
static POS_POINT m_pos_history[POS_HISTORY_LEN];
static int m_pos_history_ptr;
static bool m_pos_history_print;
static bool m_gps_corr_print;
static bool m_en_delay_comp;
static int32_t m_pps_cnt;
static nmea_gsv_info_t m_gpgsv_last;
static nmea_gsv_info_t m_glgsv_last;
static int m_print_sat_prn;
#if HAS_DIFF_STEERING
static bool m_vesc_left_now;
static mc_values m_mc_val_right;
#endif
static float m_yaw_imu_clamp;
static bool m_yaw_imu_clamp_set;
static float m_yaw_bias;
static int iCounter;
static int iCounterShow;
static float yawdrifttotal;

// Private functions
static void cmd_terminal_delay_info(int argc, const char **argv);
static void cmd_terminal_gps_corr_info(int argc, const char **argv);
static void cmd_terminal_delay_comp(int argc, const char **argv);
static void cmd_terminal_print_sat_info(int argc, const char **argv);
static void cmd_terminal_sat_info(int argc, const char **argv);
static void cmd_terminal_testGL(int argc, const char **argv);
static void cmd_terminal_setpos(int argc, const char **argv);
static void cmd_terminal_debug(int argc, const char **argv);
static void mpu9150_read(float *accel, float *gyro, float *mag);
static void update_orientation_angles(float *accel, float *gyro, float *mag, float dt);
static void init_gps_local(GPS_STATE *gps);
static void ublox_relposned_rx(ubx_nav_relposned *pos);
static void save_pos_history(void);
static POS_POINT get_closest_point_to_time(int32_t time);
static void correct_pos_gps(POS_STATE *pos);
static void ubx_rx_rawx(ubx_rxm_rawx *rawx);

int iDebug;

static void mc_values_received(mc_values *val);
static void vehicle_update_pos(float distance, float turn_rad_rear, float angle_diff, float speed);

void pos_init(void) {
	ahrs_init_attitude_info(&m_att);		// Sets initial ahrs values to zero
	m_attitude_init_done = false;			// Attitude not initiated yet
	memset(&m_pos, 0, sizeof(m_pos));		// Set vehicle position to 0s
	memset(&m_gps, 0, sizeof(m_gps));		// Set vehicle position to 0s
	memset(&m_mc_val, 0, sizeof(m_mc_val));
	m_ubx_pos_valid = true;
	m_nma_last_time = 0;					// Last time for read nmEa values = 0?
	memset(&m_pos_history, 0, sizeof(m_pos_history));
	m_pos_history_ptr = 0;
	m_pos_history_print = false;
	m_gps_corr_print = false;
	m_en_delay_comp = true;
	m_pps_cnt = 0;
	m_imu_yaw_offset = 0.0;					// No imu yaw offset to start
	memset(&m_gpgsv_last, 0, sizeof(m_gpgsv_last)); // Set last gpgsv to 0
	memset(&m_glgsv_last, 0, sizeof(m_glgsv_last)); // Set last glgsv to 0
	m_print_sat_prn = 0;
	iDebug=0;
	m_yaw_bias=0.0;
	iCounter=0;
	iCounterShow=0;
	yawdrifttotal=0.0;

#if HAS_DIFF_STEERING
	m_vesc_left_now = true;
	memset(&m_mc_val, 0, sizeof(m_mc_val));
#endif

	m_yaw_imu_clamp = 0.0;
	m_yaw_imu_clamp_set = false;

	m_ms_today = -1;
	chMtxObjectInit(&m_mutex_pos);
	chMtxObjectInit(&m_mutex_gps);

#if HAS_BMI270
	// MP101 sampling starts after all application drivers have initialized.
#elif HAS_BMI160
	commands_printf("Has BMI 160\n");
	bmi160_wrapper_init(500);
	bmi160_wrapper_set_read_callback(mpu9150_read);
#else
	commands_printf("Uses MPU 9150\n");
	mpu9150_init();							// Initiates MPU9150 (both sets values to zeros and start thread & low level inititation
	chThdSleepMilliseconds(1000);
	led_write(LED_RED, 1);
	mpu9150_sample_gyro_offsets(100);		// Reads initial values I think
	led_write(LED_RED, 0);
	mpu9150_set_read_callback(mpu9150_read);
#endif

	// Iteration timer (ITERATION_TIMER_FREQ Hz)
	TIM_TimeBaseInitTypeDef  TIM_TimeBaseStructure;
	RCC_APB1PeriphClockCmd(RCC_APB1Periph_TIM6, ENABLE);
	uint16_t PrescalerValue = (uint16_t)((168e6 / 2) / ITERATION_TIMER_FREQ) - 1;
	TIM_TimeBaseStructure.TIM_Period = 0xFFFF;
	TIM_TimeBaseStructure.TIM_Prescaler = PrescalerValue;
	TIM_TimeBaseStructure.TIM_ClockDivision = 0;
	TIM_TimeBaseStructure.TIM_CounterMode = TIM_CounterMode_Up;
	TIM_TimeBaseInit(TIM6, &TIM_TimeBaseStructure);
	TIM_Cmd(TIM6, ENABLE);

	// Set callback functions that check ublox quality (?)
	ublox_set_rx_callback_relposned(ublox_relposned_rx);
	ublox_set_rx_callback_rawx(ubx_rx_rawx);

	commands_printf("Communicate VESC\n");

	bldc_interface_set_rx_value_func(mc_values_received);

	// PPS interrupt
#if UBLOX_EN && UBLOX_USE_PPS
	extChannelEnable(&EXTD1, 8);
#elif GPS_EXT_PPS
	palSetPadMode(GPIOD, 4, PAL_MODE_INPUT_PULLDOWN);
	extChannelEnable(&EXTD1, 4);
#endif

	terminal_register_command_callback(
			"pos_delay_info",
			"Print delay information when doing GNSS position correction.\n"
			"  0 - Disabled\n"
			"  1 - Enabled",
			"[print_en]",
			cmd_terminal_delay_info);

	terminal_register_command_callback(
			"pos_gnss_corr_info",
			"Print correction information when doing GNSS position correction.\n"
			"  0 - Disabled\n"
			"  1 - Enabled",
			"[print_en]",
			cmd_terminal_gps_corr_info);

	terminal_register_command_callback(
			"pos_delay_comp",
			"Enable or disable delay compensation.\n"
			"  0 - Disabled\n"
			"  1 - Enabled",
			"[enabled]",
			cmd_terminal_delay_comp);

	terminal_register_command_callback(
			"pos_print_sat_info",
			"Print all satellite information.",
			0,
			cmd_terminal_print_sat_info);

	terminal_register_command_callback(
			"pos_print_testGL",
			"Output log data to understand how the IMU etc. work.",
			0,
			cmd_terminal_testGL);

	terminal_register_command_callback(
			"setpos",
			"Set x, y and angle",
			0,
			cmd_terminal_setpos);

	terminal_register_command_callback(
			"debug",
			"Turn on/off debug info",
			0,
			cmd_terminal_debug);

	terminal_register_command_callback(
			"pos_sat_info",
			"Print information about a satellite.\n"
			"  0 - Disabled\n"
			"  prn - satellite with prn.",
			"[prn]",
			cmd_terminal_sat_info);

	(void)save_pos_history();
	commands_printf("Done initializing! \n");
}

void pos_pps_cb(EXTDriver *extp, expchannel_t channel) {
	(void)extp;
	(void)channel;

	// Some filtering in case of long cables and noise
#if GPS_EXT_PPS
	bool high = true;
	for (int i = 0; i < 10;i++) {
		if (!palReadPad(GPIOD, 4)) {
			high = false;
		}
	}

	if (!high) {
		return;
	}
#endif

	static int32_t last_timestamp = 0;

	// Only one correction per time stamp.
	if (last_timestamp == m_nma_last_time) {
		return;
	}

	m_pps_cnt++;

	// Assume that the last NMEA time stamp is less than one second
	// old and round to the closest second after it.
	if (m_nma_last_time != 0) {
		int32_t s_today = m_nma_last_time / 1000;
		s_today++;
		m_ms_today = s_today * 1000;
	}

	last_timestamp = m_nma_last_time;
}

void pos_get_imu(float *accel, float *gyro, float *mag) {
	if (accel) {
		accel[0] = m_accel[0];
		accel[1] = m_accel[1];
		accel[2] = m_accel[2];
	}

	if (gyro) {
		gyro[0] = m_gyro[0];
		gyro[1] = m_gyro[1];
		gyro[2] = m_gyro[2];
	}

	if (mag) {
		mag[0] = m_mag_raw[0];
		mag[1] = m_mag_raw[1];
		mag[2] = m_mag_raw[2];
	}
}

void pos_get_quaternions(float *q) {
	chMtxLock(&m_mutex_pos);
	q[0] = m_pos.q0;
	q[1] = m_pos.q1;
	q[2] = m_pos.q2;
	q[3] = m_pos.q3;
	chMtxUnlock(&m_mutex_pos);
}

void pos_get_pos(POS_STATE *p) {
	chMtxLock(&m_mutex_pos);
	*p = m_pos;
	chMtxUnlock(&m_mutex_pos);
}

void pos_get_gps(GPS_STATE *p) {
	chMtxLock(&m_mutex_gps);
	*p = m_gps;
	chMtxUnlock(&m_mutex_gps);
}

float pos_get_speed(void) {
	return m_pos.speed;
}

void pos_set_xya(float x, float y, float angle) {
	commands_printf("In Pos Set XYA\n");
	commands_printf("Setting position to (x,y,z): %f, %f %f", (double) x,(double) y,(double) angle);
	chMtxLock(&m_mutex_pos);
	chMtxLock(&m_mutex_gps);
	m_pos.px = x;
	m_pos.py = y;
	m_pos.yaw = angle;
	m_imu_yaw_offset = m_pos.yaw_imu - angle;
	m_yaw_imu_clamp = angle;

	chMtxUnlock(&m_mutex_gps);
	chMtxUnlock(&m_mutex_pos);
}

void pos_set_yaw_offset(float angle) {
	chMtxLock(&m_mutex_pos);

	m_imu_yaw_offset = angle;
	utils_norm_angle(&m_imu_yaw_offset);
	m_pos.yaw = m_pos.yaw_imu - m_imu_yaw_offset;
	utils_norm_angle(&m_pos.yaw);
	m_yaw_imu_clamp = m_pos.yaw;

	chMtxUnlock(&m_mutex_pos);
}

void pos_set_enu_ref(double lat, double lon, double height) {

	double x, y, z;
	utils_llh_to_xyz(lat, lon, height, &x, &y, &z);
	chMtxLock(&m_mutex_gps); //

	m_gps.lon=lon;
	m_gps.lat=lat;

	m_gps.ix = x;
	m_gps.iy = y;
	m_gps.iz = z;

	float so = sinf((float)lon * M_PI / 180.0);
	float co = cosf((float)lon * M_PI / 180.0);
	float sa = sinf((float)lat * M_PI / 180.0);
	float ca = cosf((float)lat * M_PI / 180.0);

	// ENU
	m_gps.r1c1 = -so;
	m_gps.r1c2 = co;
	m_gps.r1c3 = 0.0;

	m_gps.r2c1 = -sa * co;
	m_gps.r2c2 = -sa * so;
	m_gps.r2c3 = ca;

	m_gps.r3c1 = ca * co;
	m_gps.r3c2 = ca * so;
	m_gps.r3c3 = sa;

	m_gps.lx = 0.0;
	m_gps.ly = 0.0;
	m_gps.lz = 0.0;

	m_gps.local_init_done = true;

	chMtxUnlock(&m_mutex_gps); //
}

void pos_get_enu_ref(double *llh) {
	chMtxLock(&m_mutex_gps);
	utils_xyz_to_llh(m_gps.ix, m_gps.iy, m_gps.iz, &llh[0], &llh[1], &llh[2]);
	chMtxUnlock(&m_mutex_gps);
}

void pos_reset_enu_ref(void) {
	m_gps.local_init_done = false;
}

int32_t pos_get_ms_today(void) {
	return m_ms_today;
}

void pos_get_mc_val(mc_values *v) {
	*v = m_mc_val;
}

void pos_set_ms_today(int32_t ms) {
	m_ms_today = ms;
}

bool pos_input_nmea(const char *data) {

	nmea_gga_info_t gga;
	static nmea_gsv_info_t gpgsv;
	static nmea_gsv_info_t glgsv;

	// decode various bits of the text string, fill various structures based on the text string
	int gga_res = utils_decode_nmea_gga(data, &gga);
	int gpgsv_res = utils_decode_nmea_gsv("GP", data, &gpgsv);
	int glgsv_res = utils_decode_nmea_gsv("GL", data, &glgsv);

	if (gpgsv_res == 1) {
		utils_sync_nmea_gsv_info(&m_gpgsv_last, &gpgsv);
	}

	if (glgsv_res == 1) {
		utils_sync_nmea_gsv_info(&m_glgsv_last, &glgsv);
	}

	if (gga.t_tow >= 0) {
		m_nma_last_time = gga.t_tow;

#if !(UBLOX_EN && UBLOX_USE_PPS) && !GPS_EXT_PPS
		m_ms_today = gga.t_tow;
#endif
	}

	// Only use valid fixes
//	if (gga.fix_type == 1 || gga.fix_type == 2 || gga.fix_type == 4 || gga.fix_type == 5) {
	if (1) {
		// Convert llh to ecef
		double sinp = sin(gga.lat * D_PI / D(180.0));
		double cosp = cos(gga.lat * D_PI / D(180.0));
		double sinl = sin(gga.lon * D_PI / D(180.0));
		double cosl = cos(gga.lon * D_PI / D(180.0));
		double e2 = FE_WGS84 * (D(2.0) - FE_WGS84);
		double v = RE_WGS84 / sqrt(D(1.0) - e2 * sinp * sinp);

		chMtxLock(&m_mutex_gps);

		m_gps.lat = gga.lat;
		m_gps.lon = gga.lon;
		m_gps.height = gga.height;
		m_gps.fix_type = gga.fix_type;
		m_gps.sats = gga.n_sat;
		m_gps.ms = gga.t_tow;
		// Convert to x,y,z
		m_gps.x = (v + gga.height) * cosp * cosl;
		m_gps.y = (v + gga.height) * cosp * sinl;
		m_gps.z = (v * (D(1.0) - e2) + gga.height) * sinp;

		// Continue if ENU frame is initialized
		if (m_gps.local_init_done) {
			float dx = (float)(m_gps.x - m_gps.ix);
			float dy = (float)(m_gps.y - m_gps.iy);
			float dz = (float)(m_gps.z - m_gps.iz);

			m_gps.lx = m_gps.r1c1 * dx + m_gps.r1c2 * dy + m_gps.r1c3 * dz;
			m_gps.ly = m_gps.r2c1 * dx + m_gps.r2c2 * dy + m_gps.r2c3 * dz;
			m_gps.lz = m_gps.r3c1 * dx + m_gps.r3c2 * dy + m_gps.r3c3 * dz;

			float px = m_gps.lx;
			float py = m_gps.ly;
			// Apply antenna offset
			const float s_yaw = sinf(-m_pos.yaw * M_PI / 180.0);
			const float c_yaw = cosf(-m_pos.yaw * M_PI / 180.0);
			px -= c_yaw * main_config.gps_ant_x - s_yaw * main_config.gps_ant_y;
			py -= s_yaw * main_config.gps_ant_x + c_yaw * main_config.gps_ant_y;
			chMtxLock(&m_mutex_pos);

			// Save last gps position
			m_pos.px_gps_last = m_pos.px_gps;
			m_pos.py_gps_last = m_pos.py_gps;
			m_pos.pz_gps_last = m_pos.pz_gps;
			m_pos.gps_ms_last = m_pos.gps_ms;

			// Set new gps position
			m_pos.px_gps = px;
			m_pos.py_gps = py;
			m_pos.pz_gps = m_gps.lz;
			m_pos.gps_ms = m_gps.ms;
			m_pos.gps_fix_type = m_gps.fix_type;

//////
			// Correct position
			// Optionally require RTK and good ublox quality indication.
/*
			if (main_config.gps_comp &&
					(!main_config.gps_req_rtk || (gga.fix_type == 4 || gga.fix_type == 5)) &&
					(!main_config.gps_use_ubx_info || m_ubx_pos_valid)) {*/
			if (main_config.gps_comp) {

				/*				if (1) {*/

				m_pos.gps_last_corr_diff = sqrtf(SQ(m_pos.px - m_pos.px_gps) +
						SQ(m_pos.py - m_pos.py_gps));

				correct_pos_gps(&m_pos); // Correct position based on vehicle angle
				m_pos.gps_corr_time = chVTGetSystemTimeX();

				m_pos.pz = m_pos.pz_gps - m_pos.gps_ground_level;

			} else
			{
				if(iDebug==4) {
				commands_printf("No compensation!");
				correct_pos_gps(&m_pos);
				commands_printf("No compensation! DONE");
				}
			}

			m_pos.gps_corr_cnt = 0.0;

			chMtxUnlock(&m_mutex_pos);
		} else {
			//If no initiation done before initiate now
			chMtxUnlock(&m_mutex_gps);
			init_gps_local(&m_gps);
			chMtxLock(&m_mutex_gps);
			m_gps.local_init_done = true;
		}
		// Read system time and update time for the gps
		m_gps.update_time = chVTGetSystemTimeX();

		chMtxUnlock(&m_mutex_gps);
	}
	return gga_res >= 0;
}

void pos_reset_attitude(void) {
	m_attitude_init_done = false;
}

/**
 * Get time since GPS correction was applied.
 *
 * @return
 * The time since GPS correction was applied in milliseconds.
 */
int pos_time_since_gps_corr(void) {
	return ST2MS(chVTTimeElapsedSinceX(m_pos.gps_corr_time));
}

void pos_base_rtcm_obs(rtcm_obs_header_t *header, rtcm_obs_t *obs, int obs_num) {
	//	commands_printf("In Pos Base RTCM\n");
	float snr = 0.0;
	float snr_base = 0.0;
	bool lock = false;
	bool lock_base = false;
	float elevation = 0.0;
	bool found = false;

	if (header->type == 1002 || header->type == 1004) {
		m_gpgsv_last.sat_num_base = obs_num;

		for (int j = 0;j < m_gpgsv_last.sat_num;j++) {
			m_gpgsv_last.sats[j].base_snr = -1;
			m_gpgsv_last.sats[j].base_lock = false;
		}

		for (int i = 0;i < obs_num;i++) {
			for (int j = 0;j < m_gpgsv_last.sat_num;j++) {
				if (m_gpgsv_last.sats[j].prn == obs[i].prn) {
					m_gpgsv_last.sats[j].base_snr = obs[i].cn0[0];
					m_gpgsv_last.sats[j].base_lock = obs[i].lock[0] == 127;

					if (m_gpgsv_last.sats[j].prn == m_print_sat_prn) {
						snr = m_gpgsv_last.sats[j].snr;
						snr_base = m_gpgsv_last.sats[j].base_snr;
						lock = m_gpgsv_last.sats[j].local_lock;
						lock_base = m_gpgsv_last.sats[j].base_lock;
						elevation = m_gpgsv_last.sats[j].elevation;
						found = true;
					}
				}
			}
		}
	} else if (header->type == 1010 || header->type == 1012) {
		m_glgsv_last.sat_num_base = obs_num;

		for (int j = 0;j < m_glgsv_last.sat_num;j++) {
			m_glgsv_last.sats[j].base_snr = -1;
			m_glgsv_last.sats[j].base_lock = false;
		}

		for (int i = 0;i < obs_num;i++) {
			for (int j = 0;j < m_glgsv_last.sat_num;j++) {
				if (m_glgsv_last.sats[j].prn == (obs[i].prn + 64)) {
					m_glgsv_last.sats[j].base_snr = obs[i].cn0[0];
					m_glgsv_last.sats[j].base_lock = obs[i].lock[0] == 127;

					if (m_glgsv_last.sats[j].prn == m_print_sat_prn) {
						snr = m_glgsv_last.sats[j].snr;
						snr_base = m_glgsv_last.sats[j].base_snr;
						lock = m_glgsv_last.sats[j].local_lock;
						lock_base = m_glgsv_last.sats[j].base_lock;
						elevation = m_glgsv_last.sats[j].elevation;
						found = true;
					}
				}
			}
		}
	}

	static int print_before = 0;
	static int sample = 0;
	if (print_before == 0) {
		sample = 0;
	}

	if (found) {
		if (m_print_sat_prn) {
			commands_printf("SNR: %.0f, SNR Base: %.0f, Lock: %d. Lock Base: %d, Elevation: %.0f",
					(double)snr, (double)snr_base, lock, lock_base, (double)elevation);

			sample++;
		}
	}
	print_before = m_print_sat_prn;
}

static void cmd_terminal_delay_info(int argc, const char **argv) {
	if (argc == 2) {
		if (strcmp(argv[1], "0") == 0) {
			m_pos_history_print = 0;
			commands_printf("OK\n");
		} else if (strcmp(argv[1], "1") == 0) {
			m_pos_history_print = 1;
			commands_printf("OK\n");
		} else {
			commands_printf("Invalid argument %s\n", argv[1]);
		}
	} else {
		commands_printf("Wrong number of arguments\n");
	}
}

static void cmd_terminal_gps_corr_info(int argc, const char **argv) {
	if (argc == 2) {
		if (strcmp(argv[1], "0") == 0) {
			m_gps_corr_print = 0;
			commands_printf("OK\n");
		} else if (strcmp(argv[1], "1") == 0) {
			m_gps_corr_print = 1;
			commands_printf("OK\n");
		} else {
			commands_printf("Invalid argument %s\n", argv[1]);
		}
	} else {
		commands_printf("Wrong number of arguments\n");
	}
}

static void cmd_terminal_delay_comp(int argc, const char **argv) {
	if (argc == 2) {
		if (strcmp(argv[1], "0") == 0) {
			m_en_delay_comp = 0;
			commands_printf("OK\n");
		} else if (strcmp(argv[1], "1") == 0) {
			m_en_delay_comp = 1;
			commands_printf("OK\n");
		} else {
			commands_printf("Invalid argument %s\n", argv[1]);
		}
	} else {
		commands_printf("Wrong number of arguments\n");
	}
}

static void cmd_terminal_print_sat_info(int argc, const char **argv) {
	(void)argc;
	(void)argv;

	commands_printf(
			"=== LOCAL ===\n"
			"Sats tot     : %d\n"
			"Sats GPS     : %d\n"
			"Sats GLONASS : %d\n"
			"=== BASE ===\n"
			"Sats tot     : %d\n"
			"Sats GPS     : %d\n"
			"Sats GLONASS : %d",
			m_gpgsv_last.sat_num + m_glgsv_last.sat_num,
			m_gpgsv_last.sat_num,
			m_glgsv_last.sat_num,
			m_gpgsv_last.sat_num_base + m_glgsv_last.sat_num_base,
			m_gpgsv_last.sat_num_base,
			m_glgsv_last.sat_num_base);

	commands_printf("====== GPS ======");

	for (int i = 0;i < m_gpgsv_last.sat_num;i++) {
		commands_printf(
				"PRN       : %d\n"
				"Elevation : %.1f\n"
				"Azimuth   : %.1f\n"
				"SNR       : %.1f\n"
				"Base SNR  : %.1f\n"
				"Base Lock : %d\n"
				"Local Lock: %d\n"
				"=========",
				m_gpgsv_last.sats[i].prn,
				(double)m_gpgsv_last.sats[i].elevation,
				(double)m_gpgsv_last.sats[i].azimuth,
				(double)m_gpgsv_last.sats[i].snr,
				(double)m_gpgsv_last.sats[i].base_snr,
				m_gpgsv_last.sats[i].base_lock,
				m_gpgsv_last.sats[i].local_lock);
	}

	commands_printf("====== GLONASS ======");

	for (int i = 0;i < m_glgsv_last.sat_num;i++) {
		commands_printf(
				"PRN       : %d\n"
				"Elevation : %.1f\n"
				"Azimuth   : %.1f\n"
				"SNR       : %.1f\n"
				"Base SNR  : %.1f\n"
				"Base Lock : %d\n"
				"Local Lock: %d\n"
				"=========",
				m_glgsv_last.sats[i].prn,
				(double)m_glgsv_last.sats[i].elevation,
				(double)m_glgsv_last.sats[i].azimuth,
				(double)m_glgsv_last.sats[i].snr,
				(double)m_glgsv_last.sats[i].base_snr,
				m_glgsv_last.sats[i].base_lock,
				m_glgsv_last.sats[i].local_lock);
	}
}

static void cmd_terminal_sat_info(int argc, const char **argv) {
	if (argc == 2) {
		int n = -1;
		sscanf(argv[1], "%d", &n);

		if (n < 0) {
			commands_printf("Invalid argument\n");
		} else {
			if (n > 0) {
				commands_printf("OK. Printing information satellite %d\n", n);
			} else {
				commands_printf("OK. Not printing satellite information.\n", n);
			}

			m_print_sat_prn = n;
		}
	} else {
		commands_printf("Wrong number of arguments\n");
	}
}

static void cmd_terminal_setpos(int argc, const char **argv) {
	float x,y,z;
	sscanf(argv[1], "%f", &x);
	sscanf(argv[2], "%f", &y);
	sscanf(argv[3], "%f", &z);
	commands_printf("x: %f\n",(double) x);
	commands_printf("y: %f\n",(double) y);
	commands_printf("z: %f\n",(double) z);
}

static void cmd_terminal_debug(int argc, const char **argv) {
	sscanf(argv[1], "%i", &iDebug);
	commands_printf("Debug: %i\n",iDebug);
}

static void cmd_terminal_testGL(int argc, const char **argv) {
	commands_printf(
			"m_mag[0]       : %f\n"
			"m_mag[1]       : %f\n"
			"m_mag[2]       : %f\n",
			(double) m_mag[0],
			(double) m_mag[1],
			(double) m_mag[2]);

	commands_printf(
			"m_mag_raw[0]       : %f\n"
			"m_mag_raw[1]       : %f\n"
			"m_mag_raw[2]       : %f\n",
			(double) m_mag_raw[0],
			(double) m_mag_raw[1],
			(double) m_mag_raw[2]);

	commands_printf(
			"m_gyro[0]       : %f\n"
			"m_gyro[1]       : %f\n"
			"m_gyro[2]       : %f\n",
			(double) m_gyro[0],
			(double) m_gyro[1],
			(double) m_gyro[2]);

	commands_printf(
			"m_accel[0]       : %f\n"
			"m_accel[1]       : %f\n"
			"m_accel[2]       : %f\n",
			(double) m_accel[0],
			(double) m_accel[1],
			(double) m_accel[2]);

	commands_printf("GPS\n"
			"latitude:       : %f\n"
			"longitude   	 : %f\n"
			"height			 : %f\n"
			"x			     : %f\n"
			"y			     : %f\n"
			"z			     : %f\n",
			(double) m_gps.lat,
			(double) m_gps.lon,
			(double) m_gps.height,
			(double) m_gps.x,
			(double) m_gps.y,
			(double) m_gps.z);

	commands_printf("POS\n"
			"roll:       : %f\n"
			"pitch   	 : %f\n"
			"yaw			 : %f\n"
			"speed			     : %f\n"
			"vx			     : %f\n"
			"vy			     : %f\n"
			"px			     : %f\n"
			"py			     : %f\n"
			"pz			     : %f\n",
			(double) m_pos.roll,
			(double) m_pos.pitch,
			(double) m_pos.yaw,
			(double) m_pos.speed,
			(double) m_pos.vx,
			(double) m_pos.vy,
			(double) m_pos.px,
			(double) m_pos.py,
			(double) m_pos.pz);
};

void broadcastisInititated(void) {
	static char print_buffer[255];

	print_buffer[0] = ID_VEHICLE_CLIENT;
	print_buffer[1] = CMD_POS_INITIATED;
	comm_usb_send_packet((unsigned char*)print_buffer, 2);
}


#if HAS_BMI270
void pos_start_imu(void) {
	// The callback polls CAN and uses TIM6: main starts it only after those
	// drivers, USB, configuration and position state are ready.
	bmi270_wrapper_set_read_callback(mpu9150_read);
	bmi270_wrapper_init(500);
}
#endif

static void mpu9150_read(float *accel, float *gyro, float *mag) {
	static unsigned int cnt_last = 0;
	volatile unsigned int cnt = TIM6->CNT;
	unsigned int time_elapsed = (cnt - cnt_last) % 65536;
	cnt_last = cnt;
	float dt = (float)time_elapsed / (float)ITERATION_TIMER_FREQ;
	/*
	commands_printf("in mpu9150_read");
	commands_printf("accel			     : %f\n"
	"gyro			     : %f\n"
	"mag			     : %f\n",
	accel,
	gyro,
	mag);
	*/

	update_orientation_angles(accel, gyro, mag, dt);

//#if MAIN_MODE == MAIN_MODE_VEHICLE
	// Read MC values every 10 iterations (should be 100 Hz)
	static int mc_read_cnt = 0;
	mc_read_cnt++;

	#if HAS_DIFF_STEERING
	if (mc_read_cnt >= 5) {
		mc_read_cnt = 0;

		comm_can_lock_vesc();
		if (m_vesc_left_now) {
			m_vesc_left_now = false;
			comm_can_set_vesc_id(DIFF_STEERING_VESC_RIGHT);
		} else {
			m_vesc_left_now = true;
			comm_can_set_vesc_id(DIFF_STEERING_VESC_LEFT);
		}
		bldc_interface_get_values();
		comm_can_unlock_vesc();
	}
	#else
//	commands_printf("has not diff sterring");
//	comm_can_set_vesc_id(36);
//	bldc_interface_get_values();

	if (mc_read_cnt >= 10) {
		mc_read_cnt = 0;
#ifdef IS_ROBANT
		// Fråga alltid första motorn i motorlistan (en drivmotor på 48 V-batteriet),
		// inte den VESC som råkar ha fått senaste kommandot. Batterispänning,
		// temperatur och felkod i RControlStation kommer härifrån.
		comm_can_lock_vesc();
		if (main_config.vehicle.actuators > 0) {
			comm_can_set_vesc_id(main_config.vehicle.actuator[0].motorid);
		}
		bldc_interface_get_values();
		comm_can_unlock_vesc();
#else
		bldc_interface_get_values();
#endif

	//	#if HAS_HYDRAULIC_DRIVE
		float turn_rad_rear = 0.0;
		float angle_diff = 0.0;
		float distance = hydraulic_get_distance(true);

		float speed = hydraulic_get_speed();

		float steering_angle = (servo_simple_get_pos_now()
				- main_config.vehicle.steering_center)
							* ((2.0 * main_config.vehicle.steering_max_angle_rad)
									/ main_config.vehicle.steering_range);


		if (fabsf(steering_angle) >= 1e-6) {
			turn_rad_rear = main_config.vehicle.axis_distance / tanf(steering_angle);
			float turn_rad_front = sqrtf(
					main_config.vehicle.axis_distance * main_config.vehicle.axis_distance
					+ turn_rad_rear * turn_rad_rear);

			if (turn_rad_rear < 0) {
				turn_rad_front = -turn_rad_front;
			}

			angle_diff = (distance * 2.0) / (turn_rad_rear + turn_rad_front);
		}
		vehicle_update_pos(distance, turn_rad_rear, angle_diff, speed);
		#endif
	}
	//#endif
//#endif

	// Update time today
	if (m_ms_today >= 0) {
		int time_elapsed_ms = time_elapsed / (ITERATION_TIMER_FREQ / 1000);

		// Accumulated error to avoid drift over time
		static int diff = 0;
		diff += time_elapsed % (ITERATION_TIMER_FREQ / 1000);
		int diff_div = diff / (ITERATION_TIMER_FREQ / 1000);
		diff -= diff_div * (ITERATION_TIMER_FREQ / 1000);

		m_ms_today += time_elapsed_ms + diff_div;

		if (m_ms_today >= MS_PER_DAY) {
			m_ms_today -= MS_PER_DAY;
		}
	}
}

static void update_orientation_angles(float *accel, float *gyro, float *mag, float dt) {

	//	commands_printf("In update orientation angles:\n");
	//  ÄR HÄR MYCKET
	gyro[0] = gyro[0] * M_PI / 180.0;
	gyro[1] = gyro[1] * M_PI / 180.0;
	gyro[2] = gyro[2] * M_PI / 180.0;

	m_mag_raw[0] = mag[0];
	m_mag_raw[1] = mag[1];
	m_mag_raw[2] = mag[2];

	/*
	 * Hard and soft iron compensation
	 *
	 * http://davidegironi.blogspot.it/2013/01/magnetometer-calibration-helper-01-for.html#.UriTqkMjulM
	 *
	 * xt_raw = x_raw - offsetx;
	 * yt_raw = y_raw - offsety;
	 * zt_raw = z_raw - offsetz;
	 * x_calibrated = scalefactor_x[1] * xt_raw + scalefactor_x[2] * yt_raw + scalefactor_x[3] * zt_raw;
	 * y_calibrated = scalefactor_y[1] * xt_raw + scalefactor_y[2] * yt_raw + scalefactor_y[3] * zt_raw;
	 * z_calibrated = scalefactor_z[1] * xt_raw + scalefactor_z[2] * yt_raw + scalefactor_z[3] * zt_raw;
	 */
	if (main_config.mag_comp) {
		float mag_t[3];

		mag_t[0] = mag[0] - main_config.mag_cal_cx;
		mag_t[1] = mag[1] - main_config.mag_cal_cy;
		mag_t[2] = mag[2] - main_config.mag_cal_cz;

		mag[0] = main_config.mag_cal_xx * mag_t[0] + main_config.mag_cal_xy * mag_t[1] + main_config.mag_cal_xz * mag_t[2];
		mag[1] = main_config.mag_cal_yx * mag_t[0] + main_config.mag_cal_yy * mag_t[1] + main_config.mag_cal_yz * mag_t[2];
		mag[2] = main_config.mag_cal_zx * mag_t[0] + main_config.mag_cal_zy * mag_t[1] + main_config.mag_cal_zz * mag_t[2];
	}

	// Swap mag X and Y to match the accelerometer
	{
		float tmp[3];
		tmp[0] = mag[1];
		tmp[1] = mag[0];
		tmp[2] = mag[2];
		mag[0] = tmp[0];
		mag[1] = tmp[1];
		mag[2] = tmp[2];
	}

	// Rotate board yaw orientation
	float rotf = 0.0;

#if !HAS_BMI160 && !HAS_BMI270
	// The MPU9250 footprint is rotated 180 degrees compared to the one
	// for the MPU9150 on our PCB. Make sure that the code behaves the
	// same regardless which one is used.
	if (mpu9150_is_mpu9250()) {
		rotf += 180.0;
	}
#endif

#ifdef BOARD_YAW_ROT
	rotf += BOARD_YAW_ROT;
#endif

	rotf *= M_PI / 180.0;
	utils_norm_angle_rad(&rotf);

	float cRot = cosf(rotf);
	float sRot = sinf(rotf);

	m_accel[0] = cRot * accel[0] + sRot * accel[1];
	m_accel[1] = cRot * accel[1] - sRot * accel[0];
	m_accel[2] = accel[2];
	m_gyro[0] = cRot * gyro[0] + sRot * gyro[1];
	m_gyro[1] = cRot * gyro[1] - sRot * gyro[0];
	m_gyro[2] = gyro[2];
	m_mag[0] = cRot * mag[0] + sRot * mag[1];
	m_mag[1] = cRot * mag[1] - sRot * mag[0];
	m_mag[2] = mag[2];

	if (!m_attitude_init_done) {
		ahrs_update_initial_orientation(m_accel, m_mag, (ATTITUDE_INFO*)&m_att);
//		m_yaw_bias=0.02;
		m_attitude_init_done = true;
	} else {
		//		ahrs_update_mahony_imu(gyro, accel, dt, (ATTITUDE_INFO*)&m_att);
		ahrs_update_madgwick_imu(m_gyro, m_accel, dt, (ATTITUDE_INFO*)&m_att);
	}

	float roll = ahrs_get_roll((ATTITUDE_INFO*)&m_att);
	float pitch = ahrs_get_pitch((ATTITUDE_INFO*)&m_att);
	float yaw = ahrs_get_yaw((ATTITUDE_INFO*)&m_att);

//	yaw+=m_yaw_bias;
	// Apply tilt compensation for magnetometer values and calculate magnetic
	// field angle. See:
	// https://cache.freescale.com/files/sensors/doc/app_note/AN4248.pdf
	// Notice that hard and soft iron compensation is applied above
	float mx = -m_mag[0];
	float my = m_mag[1];
	float mz = m_mag[2];

	float sr = sinf(roll);
	float cr = cosf(roll);
	float sp = sinf(pitch);
	float cp = cosf(pitch);

	float c_mx = mx * cp + my * sr * sp + mz * sp * cr;
	float c_my = my * cr - mz * sr;

	float yaw_mag = atan2f(-c_my, c_mx) - M_PI / 2.0;

	chMtxLock(&m_mutex_pos);

	m_pos.roll = roll * 180.0 / M_PI;
	m_pos.pitch = pitch * 180.0 / M_PI;
	m_pos.roll_rate = -m_gyro[0] * 180.0 / M_PI;
	m_pos.pitch_rate = m_gyro[1] * 180.0 / M_PI;

	main_config.mag_use=0;			// Enkel test
	if (main_config.mag_use) {
		static float yaw_now = 0;
		static float yaw_imu_last = 0;

		float yaw_imu_diff = utils_angle_difference_rad(yaw, yaw_imu_last);
		yaw_imu_last = yaw;
		yaw_now += yaw_imu_diff;

		float diff = utils_angle_difference_rad(yaw_mag, yaw_now);
		yaw_now += SIGN(diff) * main_config.yaw_mag_gain * M_PI / 180.0 * dt;
		utils_norm_angle_rad(&yaw_now);

		m_pos.yaw_imu = yaw_now * 180.0 / M_PI;
	} else {
		m_pos.yaw_imu = yaw * 180.0 / M_PI;
	}
	utils_norm_angle(&m_pos.yaw_imu);
	m_pos.yaw_rate = -m_gyro[2] * 180.0 / M_PI ;
	if ((iDebug==8))
	{
		iCounter=(iCounter+1);
		iCounterShow=(iCounterShow+1) % 50;
		yawdrifttotal+=m_pos.yaw_rate;
	}

	// Correct yaw
	{
		if (!m_yaw_imu_clamp_set) {
			m_yaw_imu_clamp = m_pos.yaw_imu - m_imu_yaw_offset;
			m_yaw_imu_clamp_set = true;
		}

		if (main_config.vehicle.clamp_imu_yaw_stationary && fabsf(m_pos.speed) < 0.05) {
			m_imu_yaw_offset = m_pos.yaw_imu - m_yaw_imu_clamp;
		} else {
			m_yaw_imu_clamp = m_pos.yaw_imu - m_imu_yaw_offset;
		}
	}

	if (main_config.vehicle.yaw_use_odometry) {
		if ((iDebug==16))
		{
		commands_printf("uses odemetry\n");
		}

		if (main_config.vehicle.yaw_imu_gain > 1e-10) {
			float ang_diff = utils_angle_difference(m_pos.yaw, m_pos.yaw_imu - m_imu_yaw_offset);

			if (ang_diff > 1.2 * main_config.vehicle.yaw_imu_gain) {
				m_pos.yaw -= main_config.vehicle.yaw_imu_gain;
				utils_norm_angle(&m_pos.yaw);
			} else if (ang_diff < -1.2 * main_config.vehicle.yaw_imu_gain) {
				m_pos.yaw += main_config.vehicle.yaw_imu_gain;
				utils_norm_angle(&m_pos.yaw);
			} else {
				// Goes here
				m_pos.yaw -= ang_diff;
				utils_norm_angle(&m_pos.yaw);
			}
		}
	} else {
		m_pos.yaw = m_pos.yaw_imu - m_imu_yaw_offset;
		utils_norm_angle(&m_pos.yaw);
	}

	if ((iDebug==6))
	{
		iCounterShow=(iCounterShow+1) % 50;
		if (iCounterShow==10)
		{
			commands_printf("mpos yaw ( %.5f )", (double) m_pos.yaw);
		}
	}

	m_pos.q0 = m_att.q0;
	m_pos.q1 = m_att.q1;
	m_pos.q2 = m_att.q2;
	m_pos.q3 = m_att.q3;

	chMtxUnlock(&m_mutex_pos);
}

static void init_gps_local(GPS_STATE *gps) {
	//Initiate GPS, which basically sets the reference frame work
	if ((iDebug==1) || (iDebug==11))
	{
		commands_printf("init_gps_local ( %.5f,%.5f )", (double) gps->lon, (double) gps->lat);

	}
	//	commands_printf("In init gps local:\n");
	gps->ix = gps->x;
	gps->iy = gps->y;
	gps->iz = gps->z;

	pos_set_enu_ref(60.06314934424684, 18.07893415029704, 0);
}

static void ublox_relposned_rx(ubx_nav_relposned *pos) {
	bool valid = true;

	if (pos->acc_n > main_config.gps_ubx_max_acc) {
		valid = false;
	} else if (pos->acc_e > main_config.gps_ubx_max_acc) {
		valid = false;
	} else if (pos->acc_d > main_config.gps_ubx_max_acc) {
		valid = false;
	} else if (!pos->carr_soln) {
		valid = false;
	} else if (!pos->fix_ok) {
		valid = false;
	} else if (!pos->rel_pos_valid) {
		valid = false;
	}

	m_ubx_pos_valid = valid;
}

static void save_pos_history(void) {
	m_pos_history[m_pos_history_ptr].px = m_pos.px;
	m_pos_history[m_pos_history_ptr].py = m_pos.py;
	m_pos_history[m_pos_history_ptr].pz = m_pos.pz;
	m_pos_history[m_pos_history_ptr].yaw = m_pos.yaw;
	m_pos_history[m_pos_history_ptr].speed = m_pos.speed;
	m_pos_history[m_pos_history_ptr].time = m_ms_today;

	m_pos_history_ptr++;
	if (m_pos_history_ptr >= POS_HISTORY_LEN) {
		m_pos_history_ptr = 0;
	}
}

static POS_POINT get_closest_point_to_time(int32_t time) {
	int32_t ind = m_pos_history_ptr > 0 ? m_pos_history_ptr - 1 : POS_HISTORY_LEN - 1;
	int32_t min_diff = abs(time - m_pos_history[ind].time);
	int32_t ind_use = ind;

	int cnt = 0;
	for (;;) {
		ind = ind > 0 ? ind - 1 : POS_HISTORY_LEN - 1;

		if (ind == m_pos_history_ptr) {
			break;
		}

		int32_t diff = abs(time - m_pos_history[ind].time);

		if (diff < min_diff) {
			min_diff = diff;
			ind_use = ind;
		} else {
			break;
		}

		cnt++;
	}

	return m_pos_history[ind_use];
}

static void correct_pos_gps(POS_STATE *pos)
{
	// Calculate age of gps data
	static int sample = 0;
	if (m_pos_history_print) {
		int32_t diff = m_ms_today - pos->gps_ms;
		commands_printf("Age: %d ms, PPS_CNT: %d", diff, m_pps_cnt);
	} else {
		sample = 0;
	}

	if(iDebug==44)
	{
		commands_printf("time gps: %f\n",(double) pos->gps_ms);
		commands_printf("time gps last corr: %f\n",(double) pos->gps_ang_corr_last_gps_ms);
	}

	// If has a speed of at least 0.5 km/h
	// Yaw = Angle on the x/y plane (i.e. the angle that is most relevant on you are on ground)
	// Angle
	if (fabsf(pos->speed * 3.6) > 0.5 || 1) {
		//Calculate yaw from gps
		float yaw_gps = -atan2f(pos->py_gps - pos->gps_ang_corr_y_last_gps,
				pos->px_gps - pos->gps_ang_corr_x_last_gps) * 180.0 / M_PI;
		if(iDebug==44)
		{
			commands_printf("yaw gps: %f\n",(double) yaw_gps);
		}
		//Get mose relevant gps position(?)
		POS_POINT closest = get_closest_point_to_time(
				(pos->gps_ms + pos->gps_ang_corr_last_gps_ms) / 2.0);
		//Get change in yaw
		float yaw_diff = utils_angle_difference(yaw_gps, closest.yaw);
		//Move m_imu_yaw_offset towards (m_imu_yaw_offset - yaw_diff), with rate depending on set gain parameter
		utils_step_towards(&m_imu_yaw_offset, m_imu_yaw_offset - yaw_diff,
				main_config.gps_corr_gain_yaw * pos->gps_corr_cnt);
	}

	// Keep m_imu_yaw_offset within -180 to +180 interval
	utils_norm_angle(&m_imu_yaw_offset);

	// Position
	float gain = main_config.gps_corr_gain_stat +
			main_config.gps_corr_gain_dyn * pos->gps_corr_cnt;

	//Same as above?
	POS_POINT closest = get_closest_point_to_time(m_en_delay_comp ? pos->gps_ms : m_ms_today);
	POS_POINT closest_corr = closest;
	// Some stuff done just for printing?
//	static int sample = 0;
	static int ms_before = 0;
	if (m_gps_corr_print) {
		float diff = utils_point_distance(closest.px, closest.py, pos->px_gps, pos->py_gps) * 100.0;

		if(iDebug==4)
		{
		commands_printf("Diff: %.1f cm, Speed: %.1f km/h, Yaw: %.1f",
				(double)diff, (double)(m_pos.speed * 3.6), (double)m_pos.yaw);
		}
		sample += pos->gps_ms - ms_before;

	} else {
		sample = 0;
	}
	ms_before = pos->gps_ms;

	if(iDebug==4)
	{
		commands_printf("gain: %f\n",(double) gain);
		commands_printf("closest px: %f, py: %f, pz: %f\n",(double) closest.px,(double) closest.py,(double) closest.pz);
		commands_printf("closest corr [bef]- px: %f, py: %f, pz: %f\n",(double) closest_corr.px,(double) closest_corr.py,(double) closest_corr.pz);
		commands_printf("DSpeed: %.1f km/h, Yaw: %.1f",
			(double)(m_pos.speed * 3.6), (double)m_pos.yaw);
	}

	// Move closest_corr towards gps position
	utils_step_towards(&closest_corr.px, pos->px_gps, gain);
	utils_step_towards(&closest_corr.py, pos->py_gps, gain);

	if(iDebug==2)
	{
	commands_printf("gain: %f\n",(double) gain);
	commands_printf("closest corr [aft]- px: %f, py: %f, pz: %f\n",(double) closest_corr.px, (double) closest_corr.py,(double) closest_corr.pz);
	commands_printf("pos [bef]- px: %f, py: %f\n", (double) pos->px, (double) pos->py);
	}
	// Move position the same amount as closest_corr was moved
	pos->px += closest_corr.px - closest.px;
	pos->py += closest_corr.py - closest.py;

	if(iDebug==2)
	{
	commands_printf("pos [aft]: %.1f m, Py: %.1f m",
			(double)pos->px, (double)pos->py);
	}

	//	#endif
	// Fill history?
	pos->gps_ang_corr_x_last_gps = pos->px_gps;
	pos->gps_ang_corr_y_last_gps = pos->py_gps;
	pos->gps_ang_corr_last_gps_ms = pos->gps_ms;
}

static void ubx_rx_rawx(ubx_rxm_rawx *rawx) {
	for (int i = 0;i < rawx->num_meas;i++) {
		ubx_rxm_rawx_obs *raw_obs = &rawx->obs[i];

		if (raw_obs->gnss_id == 0) {
			for (int j = 0;j < m_gpgsv_last.sat_num;j++) {
				if (m_gpgsv_last.sats[j].prn == raw_obs->sv_id) {
					m_gpgsv_last.sats[j].local_lock = raw_obs->locktime > 2000;
				}
			}
		} else if (raw_obs->gnss_id == 6) {
			for (int j = 0;j < m_glgsv_last.sat_num;j++) {
				if (m_glgsv_last.sats[j].prn == (raw_obs->sv_id + 64)) {
					m_glgsv_last.sats[j].local_lock = raw_obs->locktime > 2000;
				}
			}
		}
	}
}

// Define the macro to switch between old and new implementations
// Uncomment the line below to use the new implementation
#define MISTRAL

#if MAIN_MODE == MAIN_MODE_VEHICLE
static void mc_values_received(mc_values *val) {
	#if HAS_DIFF_STEERING
		if (val->vesc_id == DIFF_STEERING_VESC_RIGHT || !m_vesc_left_now) {
			m_mc_val_right = *val;
			return;
		}
	#endif

    m_mc_val = *val;

	#if !WHEEL_SENSOR
    static float last_tacho = 0;
    static bool tacho_read = false;

	// New implementation
	static float last_tacho_time = 0;

	// Calculate time_last from the tachometer readings
	float time_last = fmaxf(io_board_adc0_cnt.high_time_current, io_board_adc0_cnt.high_time_last) +
					  fmaxf(io_board_adc0_cnt.low_time_current, io_board_adc0_cnt.low_time_last);

	// Calculate RPM based on time_last
	float rpm = 0;
	if (time_last > 0) {
		// Calculate the frequency of the tachometer pulses
		float frequency = 1.0 / (time_last * 1e-6); // Convert time_last to seconds

		// Calculate RPM (assuming one pulse per revolution, adjust as needed)
		rpm = frequency * 60.0;
	}

	// Reset tacho_time the first time
	if (!tacho_read) {
		tacho_read = true;
		last_tacho_time = time_last;
	}

	// Calculate distance based on RPM and vehicle properties
	float distance = (rpm * main_config.vehicle.gear_ratio * (2.0 / main_config.vehicle.motor_poles) * (1.0 / 60.0) * main_config.vehicle.wheel_diam * M_PI) * (time_last - last_tacho_time) * 1e-6;
	last_tacho_time = time_last;


    float angle_diff = 0.0;
    float turn_rad_rear = 0.0;

	#if HAS_DIFF_STEERING && !defined(MISTRAL)
    float distance_diff = (tacho_diff - last_tacho_diff)
            * main_config.vehicle.gear_ratio * (2.0 / main_config.vehicle.motor_poles)
            * (1.0 / 6.0) * main_config.vehicle.wheel_diam * M_PI;
    last_tacho_diff = tacho_diff;

    const float d1 = distance - distance_diff / 2.0;
    const float d2 = distance + distance_diff / 2.0;

    if (fabsf(d2 - d1) > 1e-6) {
        turn_rad_rear = main_config.vehicle.axis_distance * (d2 + d1) / (2 * (d2 - d1));
        angle_diff = (d2 - d1) / main_config.vehicle.axis_distance;
        utils_norm_angle_rad(&angle_diff);
    }
	#elif !defined(MISTRAL)
    float steering_angle = (servo_simple_get_pos_now()
            - main_config.vehicle.steering_center)
            * ((2.0 * main_config.vehicle.steering_max_angle_rad)
                    / main_config.vehicle.steering_range);

    if (fabsf(steering_angle) >= 1e-6) {
        turn_rad_rear = main_config.vehicle.axis_distance / tanf(steering_angle);
        float turn_rad_front = sqrtf(
                main_config.vehicle.axis_distance * main_config.vehicle.axis_distance
                + turn_rad_rear * turn_rad_rear);

        if (turn_rad_rear < 0) {
            turn_rad_front = -turn_rad_front;
        }

        angle_diff = (distance * 2.0) / (turn_rad_rear + turn_rad_front);
    }
	#endif

    float speed = rpm * main_config.vehicle.gear_ratio
            * (2.0 / main_config.vehicle.motor_poles) * (1.0 / 60.0)
            * main_config.vehicle.wheel_diam * M_PI;
    vehicle_update_pos(distance, turn_rad_rear, angle_diff, speed);
#endif
}

static void vehicle_update_pos(float distance, float turn_rad_rear, float angle_diff, float speed)
{
	chMtxLock(&m_mutex_pos);
	if (fabsf(distance) > 2) { distance=0; };

	if (fabsf(distance) > 1e-6) {
		float angle_rad = -m_pos.yaw * M_PI / 180.0;

		m_pos.gps_corr_cnt += fabsf(distance);

		if (!main_config.vehicle.yaw_use_odometry || fabsf(angle_diff) < 1e-6) {
			m_pos.px += cosf(angle_rad) * distance;
			m_pos.py += sinf(angle_rad) * distance;
		} else {
			m_pos.px += turn_rad_rear * (sinf(angle_rad + angle_diff) - sinf(angle_rad));
			m_pos.py += turn_rad_rear * (cosf(angle_rad - angle_diff) - cosf(angle_rad));
			angle_rad += angle_diff;
			utils_norm_angle_rad(&angle_rad);

			m_pos.yaw = -angle_rad * 180.0 / M_PI;
			utils_norm_angle(&m_pos.yaw);
		}
	}
	m_pos.speed = speed;

	pos_uwb_update_dr(m_pos.yaw_imu, m_pos.yaw, distance, turn_rad_rear, m_pos.speed);
	save_pos_history();

	chMtxUnlock(&m_mutex_pos);
}

#endif
