#ifndef BLDC_INTERFACE_FWVER_H_
#define BLDC_INTERFACE_FWVER_H_

#include <stdint.h>
#include <stdbool.h>

// Full info from COMM_FW_VERSION (modern firmware 5.x/6.x).
// Fields beyond minor exist only in newer firmware; see fw_is_legacy.
typedef struct {
	int fw_major;					// -1 = no reply / unknown format
	int fw_minor;
	bool fw_is_legacy;				// true = old 2-byte format (pre ~4.x)
	char hw_name[32];				// null-terminated, e.g. "VESC 6 MKIII"
	uint8_t uuid[12];				// STM32 UID
	bool pairing_done;
	uint8_t test_version_number;	// 0 = release build
	uint8_t hw_type;
	uint8_t custom_cfg_num;
	uint8_t phase_filters;
	uint8_t qmlui_hw_flags;
	uint8_t qmlui_app_flags;
	uint8_t qmlui_flags;
	uint8_t nrf_flags;
	char fw_name[24];				// null-terminated
	uint32_t hw_crc;
} fw_info;

// Latest received info (fw_major = -1 until first reply).
const fw_info* bldc_interface_get_fw_info(void);

// Version-branched parsing helper: true if firmware >= major.minor
static inline bool fw_info_is_at_least(const fw_info *i, int major, int minor) {
	if (i->fw_major < 0) {
		return false;
	}
	return (i->fw_major > major) || (i->fw_major == major && i->fw_minor >= minor);
}

// New callback; the old rx_fw_func(major, minor) is kept unchanged.
void bldc_interface_set_rx_fw_info_func(void(*func)(const fw_info *info));

#endif /* BLDC_INTERFACE_FWVER_H_ */
