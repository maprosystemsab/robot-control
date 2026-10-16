#include "ch.h"
#include "hal.h"
#include "conf_general.h"
#include <math.h>
#include "commands.h"
#include "utils.h"
#include "hydraulic.h"
#include "ch.h"
#include "hal.h"
#include "conf_general.h"
#include "hydraulic.h"
#include "motor_control.h"

#define SPEED_BUFFER_LEN   6      // antal senaste mätningar att använda i medel
#define MAX_FACTOR_CHANGE  2.0f   // tillåten förändring mellan mätningar (högsta/lägsta tid)
#define MIN_FACTOR_CHANGE  0.5f   // lägsta tillåten förändring (för kort tid)

static float time_buffer[SPEED_BUFFER_LEN];
static int buf_index = 0;
int buf_count = 0; // hur många giltiga värden i bufferten
float last_valid_time = 0.0f;
float m_speed_filtered = 0.0f;
float m_speed_pwm = 0;
extern systime_t last_reading_time;

extern volatile float m_speed_now;
extern float m_speed_pwm;
extern float m_speed_filtered;
extern float last_valid_time;
extern int buf_count;
#define SPEED_TIMEOUT_MS   400

volatile bool new_pulse = false;

extern float m_throttle_set;

extern float debugvalue;
extern float debugvalue2;
extern float debugvalue3;
extern float debugvalue4;
extern float debugvalue5;

#ifdef IS_MACTRAC
	// Measure speed
	const float wheel_diam = 0.65;
	const float cnts_per_rev = 16.0;
#endif
#if defined(WHEELSPEED_DIAM)
	// Measure speed (mått i conf_general.h, t.ex. RobAnt)
	const float wheel_diam = WHEELSPEED_DIAM;
	const float cnts_per_rev = WHEELSPEED_CNTS_PER_REV;
#elif defined(IS_DRANGEN)
	// Measure speed
	const float wheel_diam = 0.30;
	const float cnts_per_rev = 16.0;
#endif

static THD_WORKING_AREA(wheelspeed_thread_wa, 1024);
static THD_FUNCTION(wheelspeed_thread, arg);

void wheelspeed_init(void) {

	chThdCreateStatic(wheelspeed_thread_wa, sizeof(wheelspeed_thread_wa), NORMALPRIO, wheelspeed_thread, NULL);
}

void update_speed_buffer(float period, float unused) {
    // Validate input
    if (period <= 0.0f || isnan(period) || isinf(period)) {
        return;
    }

    float time_last = period;
    
    // Check for division by zero in speed calculation
    if (time_last <= 0.0001f) {  // Minimum reasonable period (100us)
        return;
    }

    // Dynamic tolerances based on speed
    float max_factor = MAX_RATIO;
    float min_factor = MIN_RATIO;
    if (m_speed_filtered < LOWSPEED) {
        max_factor = MAX_RATIO_LOWSPEED;
        min_factor = MIN_RATIO_LOWSPEED;
    }

    // Validate ratio if we have previous valid time
    if (last_valid_time > 0.0f) {
        float ratio = time_last / last_valid_time;
        if (ratio <= 0.0f || isinf(ratio)) {
            return;
        }
        if ((m_speed_filtered > 0.0f) && (ratio > max_factor || ratio < min_factor)) {
            return;
        }
    }

    // Update buffer with thread-safe operations
    chSysLockFromISR();
    last_valid_time = time_last;
    time_buffer[buf_index] = time_last;
    buf_index = (buf_index + 1) % SPEED_BUFFER_LEN;
    if (buf_count < SPEED_BUFFER_LEN) {
        buf_count++;
    }
    chSysUnlockFromISR();

    // Calculate average time
    float sum_time = 0.0f;
    int valid_count = 0;
    for (int i = 0; i < buf_count; i++) {
        if (time_buffer[i] > 0.0f) {
            sum_time += time_buffer[i];
            valid_count++;
        }
    }

    if (valid_count == 0) {
        return;
    }

    float avg_time = sum_time / valid_count;
    
    // Calculate speed with safety checks
    if (avg_time <= 0.0001f) {
        m_speed_filtered = 0.0f;
        m_speed_pwm = 0.0f;
        return;
    }

    int direction=motor_get_direction();
    // 0 = vändning på stället (RobAnt): hjulen snurrar men roboten står kvar.
    float new_speed = direction == 0 ? 0.0f : SIGN(direction) * (wheel_diam * M_PI) / (avg_time * cnts_per_rev);
    
    // Update shared speed variables
    chSysLockFromISR();
    const float alpha = 0.3f;
    m_speed_filtered = m_speed_filtered * (1.0f - alpha) + new_speed * alpha;
    m_speed_pwm = m_speed_filtered;
    chSysUnlockFromISR();
};



static THD_FUNCTION(wheelspeed_thread, arg) {
	(void)arg;

	chRegSetThreadName("WheelSpeed");
    
	/*
    // Increase stack size check
    if (chThdGetSelfX()->p_ctx != NULL) {
        if (chThdGetSelfX()->p_ctx->sp < (void*)((uint32_t)wheelspeed_thread_wa + sizeof(wheelspeed_thread_wa) - 256)) {
            commands_printf("WARNING: WheelSpeed thread low on stack!\n");
        }
    }*/
    
    while (!chThdShouldTerminateX()) {
        chThdSleepMilliseconds(50);
        
        // Timeout → nolla
        if (chVTTimeElapsedSinceX(last_reading_time) > MS2ST(SPEED_TIMEOUT_MS)) {
            chSysLock();
            buf_count = 0;
            last_valid_time = 0.0f;
            m_speed_now = 0.0f;
            m_speed_filtered = 0.0f;
            chSysUnlock();
        }

        // Om ny puls från ISR
        if (new_pulse) {
            chSysLock();
            new_pulse = false;
            m_speed_now = m_speed_pwm;
            chSysUnlock();
        }
    }
}

