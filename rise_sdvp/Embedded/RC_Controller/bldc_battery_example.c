/*
 * Example: wire up battery_level in RC_Controller (not compiled automatically through MAKEFILE).
 *
 * No polling: another variant already counts Ah/Wh during operation,
 * so the battery level only needs to be fetched at startup (after the
 * version handshake) and on demand. Register the callback once at init,
 * call battery_fetch_level() when needed, and read the value with the
 * battery_get_* functions (-1 until the first reply arrives).
 * 
 */

#include "ch.h"
#include "hal.h"
#include "bldc_interface.h"
#include "bldc_interface_fwver.h"
#include <stdint.h>
#include <stdbool.h>

static float m_battery_level = -1.0;   // -1 = no value received yet
static float m_wh_batt_left  = 0.0;

static void rx_setup_values(mc_setup_values *values) {
	chSysLock();
	m_battery_level = values->battery_level;
	m_wh_batt_left  = values->wh_batt_left;
	chSysUnlock();
}

float battery_get_level(void) {
	float v;
	chSysLock();
	v = m_battery_level;
	chSysUnlock();
	return v;
}

float battery_get_percent(void) {
	float level = battery_get_level();
	if (level < 0.0) {
		return -1.0;
	}

	float pct = level * 100.0;
	if (pct > 100.0) pct = 100.0;
	if (pct < 0.0)   pct = 0.0;

	return pct;
}

float battery_get_wh_left(void) {
	float v;
	chSysLock();
	v = m_wh_batt_left;
	chSysUnlock();
	return v;
}

// Call ONCE at startup, after communication has been initialized
void battery_example_init(void) {
	bldc_interface_set_rx_setup_value_func(rx_setup_values);
}

// Fetch battery level on demand (e.g. at startup after the version
// handshake, or when a battery view is opened). The value is delivered
// asynchronously to rx_setup_values; read it with battery_get_percent().
// Returns false if the version handshake has not completed yet.
bool battery_fetch_level(void) {
	const fw_info *fw = bldc_interface_get_fw_info();
	if (fw->fw_major < 0) {
		return false;  // no COMM_FW_VERSION reply yet
	}

	// Only bit 8 (battery_level) and bit 19 (wh_batt_left) - minimal packet
	const uint32_t mask = ((uint32_t)1 << 8) | ((uint32_t)1 << 19);
	bldc_interface_get_setup_values_selective(mask);
	return true;
}
