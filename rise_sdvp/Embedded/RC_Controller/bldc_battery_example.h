#ifndef BLDC_BATTERY_EXAMPLE_H_
#define BLDC_BATTERY_EXAMPLE_H_

#include <stdbool.h>

void battery_example_init(void);

float battery_get_level(void);    // 0.0 - 1.0 (-1 = no reply yet)
float battery_get_percent(void);  // 0 - 100 (-1 = no reply yet)
float battery_get_wh_left(void);

// Fetch on demand instead of polling:
bool battery_fetch_level(void);   // false if version handshake not done

#endif /* BLDC_BATTERY_EXAMPLE_H_ */
