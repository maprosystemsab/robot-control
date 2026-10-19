

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

#include "pwm_esc.h"
#include "ch.h"
#include "hal.h"
#include "conf_general.h"
#include "stm32f4xx_conf.h"
#include "pwm_esc.h"
#include "icu.h"

#include "chtypes.h"
#include "chsys.h"
#include "chvt.h"
#include <math.h>
#include "commands.h"
#include "wheelspeed.h"


// #include "io_board_adc.h"  // or wherever ADC_CNT_t is defined

//extern ADC_CNT_t io_board_adc0_cnt;




#ifdef SERVO_READ
//#include <time.h> // For time functions
#define TACHO_INPUT_PORT      GPIOA
ADC_CNT_t io_board_adc0_cnt = {1};
systime_t last_reading_time = 0;
#ifndef IS_ROVMCU
static volatile uint32_t timer_overflow_count = 0; // antal overflow
#endif
static volatile uint32_t last_capture = 0;         // 32-bitars senaste capture
#if defined(WHEELSPEED_MIN_LEVEL_S) && !defined(IS_ROVMCU)
// Avstudsning: en nivå på givaren räknas först när den legat still WHEELSPEED_MIN_LEVEL_S.
static volatile uint32_t wl_edge_time = 0;     // senaste flank (capture32)
static volatile bool wl_edge_level = false;    // nivån efter senaste flanken
static volatile bool wl_stable_level = false;  // senast godkända nivå
#endif
static systime_t last_tick = 0;
#ifdef IS_ROVMCU
static bool capture_started = false;
#endif
//extern float time_last;


#endif

extern int iDebug;


#ifdef PWMTEST
// Pin definition for PA2
#define TACHO_INPUT_PAD       2

static adcsample_t samplePWM = 0;
static ADCConversionGroup adcgrpcfg;

#endif

// Settings
#define ESC_UPDATE_RATE			200	// Hz
#ifdef IS_ROVMCU
// 1 MHz divides both APB timer clocks exactly (84/168 MHz) and gives 1 us
// pulse resolution. Legacy's 5 MHz does not and truncates the prescaler.
#define TIM_CLOCK               1000000U
#else
#define TIM_CLOCK				5e6 // Hz
#endif
#define ALL_CHANNELS			0xFF

// Pins
#define SERVO1_GPIO				GPIOB
#define SERVO1_PIN				0
#define SERVO2_GPIO				GPIOB
#define SERVO2_PIN				1
#if IS_F9_BOARD
#define SERVO3_GPIO				GPIOA
#define SERVO3_PIN				2
#define SERVO4_GPIO				GPIOA
#define SERVO4_PIN				3
#else
#define SERVO3_GPIO				GPIOE
#define SERVO3_PIN				5
#define SERVO4_GPIO				GPIOE
#define SERVO4_PIN				6
#endif


#ifndef CORTEX_PRIORITY_BITS
#define CORTEX_PRIORITY_BITS 4
#endif

#ifndef CORTEX_PRIORITY_MASK
#define CORTEX_PRIORITY_MASK(n) ((n) << (8 - CORTEX_PRIORITY_BITS))
#endif

static PWMConfig pwmcfg3 = {
		TIM_CLOCK,
		(uint16_t)((uint32_t)TIM_CLOCK / (uint32_t)ESC_UPDATE_RATE),
		NULL,
		{
				{PWM_OUTPUT_DISABLED, NULL},
				{PWM_OUTPUT_DISABLED, NULL},
				{PWM_OUTPUT_ACTIVE_HIGH, NULL},
				{PWM_OUTPUT_ACTIVE_HIGH, NULL}
		},
		0,
		0,
#if STM32_PWM_USE_ADVANCED
		0
#endif
};

static PWMConfig pwmcfg9 = {
		TIM_CLOCK,
		(uint16_t)((uint32_t)TIM_CLOCK / (uint32_t)ESC_UPDATE_RATE),
		NULL,
		{
				{PWM_OUTPUT_ACTIVE_HIGH, NULL},
#ifdef IS_ROVMCU
				{PWM_OUTPUT_DISABLED, NULL}, // PA3 belongs to TIM2_CH4 wheel input
#else
				{PWM_OUTPUT_ACTIVE_HIGH, NULL},
#endif
				{PWM_OUTPUT_DISABLED, NULL},
				{PWM_OUTPUT_DISABLED, NULL}
		},
		0,
		0,
#if STM32_PWM_USE_ADVANCED
		0
#endif
};

void pwm_esc_init(void) {
#ifdef SERVO_WRITE
	pwmStart(&PWMD3, &pwmcfg3);
	pwmStart(&PWMD9, &pwmcfg9);

	palSetPadMode(SERVO1_GPIO, SERVO1_PIN,
			PAL_MODE_ALTERNATE(GPIO_AF_TIM3) |
			PAL_STM32_OTYPE_PUSHPULL |
			PAL_STM32_OSPEED_MID1);
	palSetPadMode(SERVO2_GPIO, SERVO2_PIN,
			PAL_MODE_ALTERNATE(GPIO_AF_TIM3) |
			PAL_STM32_OTYPE_PUSHPULL |
			PAL_STM32_OSPEED_MID1);
	palSetPadMode(SERVO3_GPIO, SERVO3_PIN,
			PAL_MODE_ALTERNATE(GPIO_AF_TIM9) |
			PAL_STM32_OTYPE_PUSHPULL |
			PAL_STM32_OSPEED_MID1);
// PA3 är hjulpulsingång på MP101 och analog ingång för vinkelgivaren (adconv.c) med ANGLE_SENSOR_PA3.
#if !defined(IS_ROVMCU) && !defined(ANGLE_SENSOR_PA3)
	palSetPadMode(SERVO4_GPIO, SERVO4_PIN,
			PAL_MODE_ALTERNATE(GPIO_AF_TIM9) |
			PAL_STM32_OTYPE_PUSHPULL |
			PAL_STM32_OSPEED_MID1);
#endif

	pwm_esc_set_all(0);
#endif
#if defined(SERVO_READ) && !defined(IS_ROVMCU)
	// MP101 wheel capture is initialized once by main.c, independently of PWM.
	tach_input_init();
#endif
#ifdef PWMTEST
	adcgrpcfg.circular     = FALSE;
	adcgrpcfg.num_channels = 1;
	adcgrpcfg.end_cb       = NULL;
	adcgrpcfg.error_cb     = NULL;
	adcgrpcfg.cr1          = 0;
	adcgrpcfg.cr2          = ADC_CR2_SWSTART;
	adcgrpcfg.smpr1 = 0;       // Channel 2 isn't in SMPR1
	adcgrpcfg.smpr2 = 6 << 6;  // Set sample time for channel 2 (bits 8:6)
	adcgrpcfg.sqr1         = ADC_SQR1_NUM_CH(1);
	adcgrpcfg.sqr2         = 0;
	adcgrpcfg.sqr3         = ADC_SQR3_SQ1_N(ADC_CHANNEL_IN2);


	  adcStart(&ADCD1, NULL);  // Start ADC driver

    // Configure the pin as input with pull-down
    palSetPadMode(TACHO_INPUT_PORT, TACHO_INPUT_PAD, PAL_MODE_INPUT_PULLDOWN);

    // Start a debug thread
    //static THD_WORKING_AREA(waTachoDebug, 128);
    //chThdCreateStatic(waTachoDebug, sizeof(waTachoDebug), NORMALPRIO, tacho_debug_thread, NULL);


#endif
}

void pwm_esc_set_all(float pulse_width) {
	pwm_esc_set(ALL_CHANNELS, pulse_width);
}


void pwm_esc_set(uint8_t channel, float pulse_width) {
	uint32_t cnt_val;

	if (0) {
		// Always set zero if emergency stop is set.
		// TODO: Implement this
		cnt_val = ((TIM_CLOCK / 1e3) * (uint32_t)main_config.mr.motor_pwm_min_us) / 1e3;
	} else {
		cnt_val = ((TIM_CLOCK / 1e3) * (uint32_t)main_config.mr.motor_pwm_min_us) / 1e3 +
				((TIM_CLOCK / 1e3) * (uint32_t)(pulse_width * (float)(main_config.mr.motor_pwm_max_us -
						main_config.mr.motor_pwm_min_us))) / 1e3;
	}

	switch(channel) {
	case 0:
		pwmEnableChannel(&PWMD3, 2, cnt_val);
		break;

	case 1:
		pwmEnableChannel(&PWMD3, 3, cnt_val);
		break;

	case 2:
		pwmEnableChannel(&PWMD9, 0, cnt_val);
		break;

	case 3:
#ifndef IS_ROVMCU
		pwmEnableChannel(&PWMD9, 1, cnt_val);
#endif
		break;

	case ALL_CHANNELS:
		pwmEnableChannel(&PWMD3, 2, cnt_val);
		pwmEnableChannel(&PWMD3, 3, cnt_val);
		pwmEnableChannel(&PWMD9, 0, cnt_val);
#ifndef IS_ROVMCU
		pwmEnableChannel(&PWMD9, 1, cnt_val);
#endif
		break;

	default:
		break;
	}
}

void tach_input_init(void) {

    // Enable TIM2 clock
    rccEnableTIM2(TRUE);

#ifdef IS_ROVMCU
    // PA3, TIM2_CH4 (AF1), via U40 with DIR PD7 low. External R127 supplies
    // the pull-up. TIM2's full 32-bit counter avoids lost 16-bit overflows.
    palSetPadMode(TACHO_INPUT_PORT, 3, PAL_MODE_ALTERNATE(1));
    TIM2->CR1 = 0;
    TIM2->DIER = 0;
    TIM2->PSC = STM32_TIMCLK1 / 100000U - 1U; // 10 us, same units as legacy
    TIM2->ARR = 0xFFFFFFFFU;
    TIM2->CCMR2 = TIM_CCMR2_CC4S_0;
    TIM2->CCER = TIM_CCER_CC4E; // rising edges
    TIM2->EGR = TIM_EGR_UG;
    TIM2->CNT = 0;
    TIM2->SR = 0;
    TIM2->DIER = TIM_DIER_CC4IE;
    TIM2->CR1 = TIM_CR1_CEN;
    capture_started = false;
#else
    // Configure PA2 as TIM2_CH3 (AF1), unchanged on legacy boards.
#ifdef WHEELSPEED_PULLUP
    palSetPadMode(TACHO_INPUT_PORT, 2, PAL_MODE_ALTERNATE(1) | PAL_STM32_PUDR_PULLUP);
#else
    palSetPadMode(TACHO_INPUT_PORT, 2, PAL_MODE_ALTERNATE(1));
#endif

    // Timer configuration
    TIM2->PSC = 840 - 1;        // 84 MHz / 84 = 1 MHz → 1 µs per count
    TIM2->ARR = 0xFFFF;        // Max period
    TIM2->CCMR2 &= ~TIM_CCMR2_CC3S;
    TIM2->CCMR2 |= TIM_CCMR2_CC3S_0;  // CC3 input, map to TI3
#ifdef WHEELSPEED_MIN_PERIOD_S
    TIM2->CCMR2 |= TIM_CCMR2_IC3F;    // digitalt filter: kanten måste ligga stilla 8 sampel (fDTS/32)
#endif
    TIM2->CCER &= ~TIM_CCER_CC3P;     // Rising edge (clear bit for rising edge)
#ifdef WHEELSPEED_MIN_LEVEL_S
    TIM2->CCER |= TIM_CCER_CC3P | TIM_CCER_CC3NP;   // båda flankerna: avstudsningen i avbrottet
    wl_edge_level = wl_stable_level = palReadPad(TACHO_INPUT_PORT, 2);
#endif
    TIM2->CCER |= TIM_CCER_CC3E;      // Enable input capture
    
    // Try capturing falling edges instead
    // TIM2->CCER |= TIM_CCER_CC3P;  // Uncomment to capture falling edge

    // Enable interrupt on CC3 match
    TIM2->DIER |= TIM_DIER_CC3IE;
    TIM2->CR1 |= TIM_CR1_CEN;         // Start the timer
#endif

    // Enable TIM2 IRQ in NVIC
    nvicEnableVector(STM32_TIM2_NUMBER, CORTEX_PRIORITY_MASK(7));

    last_capture = 0;

    // Initialize the state machine for the first pulse
    io_board_adc0_cnt.is_high = false;
    io_board_adc0_cnt.high_time_last = 0.0f;
    io_board_adc0_cnt.high_time_current = 0.0f;
    io_board_adc0_cnt.low_time_last = 0.0f;
    io_board_adc0_cnt.low_time_current = 0.0f;
    io_board_adc0_cnt.toggle_high_cnt = 0;
    io_board_adc0_cnt.toggle_low_cnt = 0;

    // Debug: Print initialization complete
    commands_printf("Tachometer input initialized\n");
    commands_printf("TIM2->CCER = 0x%08X\n", TIM2->CCER);
    commands_printf("TIM2->DIER = 0x%08X\n", TIM2->DIER);
}

CH_IRQ_HANDLER(STM32_TIM2_HANDLER) {
    CH_IRQ_PROLOGUE();
#ifdef IS_ROVMCU
    if (TIM2->SR & TIM_SR_CC4IF) {
        uint32_t capture = TIM2->CCR4;
        uint32_t delta = capture - last_capture;
        last_capture = capture;
        last_reading_time = chVTGetSystemTimeX();
        // Ignore the first edge: it has no preceding pulse to measure against.
        if (capture_started) {
            update_speed_buffer(0.00001f * delta, 0.0f);
            new_pulse = true;
        }
        capture_started = true;
        TIM2->SR &= ~TIM_SR_CC4IF;
    }
#else
    if (TIM2->SR & TIM_SR_UIF) {
        TIM2->SR &= ~TIM_SR_UIF; // rensa flagga
        timer_overflow_count++;
    }

    if (TIM2->SR & TIM_SR_CC3IF) {
        uint16_t capture16 = TIM2->CCR3;
        uint32_t capture32 = ((uint32_t)timer_overflow_count << 16) | capture16;

#ifdef WHEELSPEED_MIN_LEVEL_S
        // Störspikar från styrmotorn är µs långa, riktiga hål/plåt minst ca 10 ms
        // (15 km/h). En stigande flank räknas först när den höga nivån legat still
        // WHEELSPEED_MIN_LEVEL_S, dvs. vid nästa flank; pulsens tid är ändå den
        // stigande flankens. Mk1 2026-10-19 med bara 30.21-filtret: 11-21 km/h i
        // svängar, bilen på kartan sprang 10 m före GPS.
        bool level = palReadPad(TACHO_INPUT_PORT, 2);
        bool count_rise = false;
        uint32_t edge_time = capture32;
        // Samma nivå som förut: en spik vars båda flanker gav ett enda avbrott.
        if (level != wl_edge_level) {
            if ((edge_time - wl_edge_time) >= (uint32_t)(WHEELSPEED_MIN_LEVEL_S * 100000.0) &&
                    wl_edge_level != wl_stable_level) {
                wl_stable_level = wl_edge_level;
                if (wl_stable_level) {
                    count_rise = true;
                    capture32 = wl_edge_time;
                }
            }
            wl_edge_time = edge_time;
            wl_edge_level = level;
        }
        if (count_rise) {
#endif
        uint32_t delta = capture32 - last_capture;
#ifdef WHEELSPEED_MIN_PERIOD_S
        // Störpuls (t.ex. från styrmotorn): räkna som om den aldrig kom.
        if (delta >= (uint32_t)(WHEELSPEED_MIN_PERIOD_S * 100000.0)) {
#endif
        last_capture = capture32;

        last_reading_time = chVTGetSystemTimeX();

        // Simplified: Just measure the time between rising edges (period)
        // This is simpler and more reliable than trying to measure both high and low times
 //       debugvalue2=delta;
        float period = 0.00001f * delta;  // Convert to seconds
        
        // Update speed buffer with the period
        // For a square wave, the period is the sum of high and low times
        update_speed_buffer(period, 0.0f);
        new_pulse = true;  // signalera till tråden
#ifdef WHEELSPEED_MIN_PERIOD_S
        }
#endif
#ifdef WHEELSPEED_MIN_LEVEL_S
        }
#endif

        TIM2->SR &= ~TIM_SR_CC3IF;
    }
#endif

    CH_IRQ_EPILOGUE();
}

#ifdef PWMTEST
void read_adc_test(void) {
		adcConvert(&ADCD1, &adcgrpcfg, &samplePWM, 1);
//		commands_printf("Raw ADC value: %d", samplePWM);

}


static void tacho_debug_thread(void *arg) {
    (void)arg;
    chRegSetThreadName("tacho_debug");
    palSetPadMode(GPIOA, 2, PAL_MODE_INPUT_ANALOG); // Set PA2 to analog input

    while (true) {
        read_adc_test();
        chThdSleepMilliseconds(1000); // Adjust as needed
    }
}
#endif
