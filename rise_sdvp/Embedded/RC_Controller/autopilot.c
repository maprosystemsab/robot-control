/*
	Copyright 2016 - 2019 Benjamin Vedder	benjamin@vedder.se

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

#include "autopilot.h"
#include <math.h>
#include <string.h>
#include <stdio.h>
#include "ch.h"
#include "hal.h"
#include "servo_simple.h"
#include "utils.h"
#include "pos.h"
#include "bldc_interface.h"
#include "motor_control.h"
#include "commands.h"
#include "terminal.h"
#include "comm_can.h"
#include "hydraulic.h"
#include "pos_uwb.h"
#include "attributes_masks.h"
//#include "servo_vesc.h"
// Defines
#define AP_HZ						100 // Hz

// Autopilot Module Overview:
// This module implements autonomous navigation for the RC Controller.
// It allows the vehicle to follow a predefined route consisting of waypoints (ROUTE_POINTs).
// The autopilot calculates steering angles and speed to navigate through the route points.
//
// Key Features:
// - Route management (add, remove, clear, replace points)
// - Dynamic radius calculation based on speed
// - Circle-line intersection for path following
// - Support for both time-based and distance-based navigation modes
// - Route repetition capability
// - Speed override functionality
// - Integration with positioning systems (GPS, UWB)
// - Hydraulic control support (when HAS_HYDRAULIC_DRIVE is enabled)
// - Differential steering support (when HAS_DIFF_STEERING is enabled)
//
// The autopilot runs in a separate thread at AP_HZ frequency (100Hz).

// Private variables
static THD_WORKING_AREA(ap_thread_wa, 2048); // Thread working area for autopilot thread (2048 bytes stack)
__attribute__((section(".ram4"))) static ROUTE_POINT m_route[AP_ROUTE_SIZE]; // Route storage in RAM4 section (2000 points max)
bool m_is_active; // Flag indicating if autopilot is currently active
static bool m_is_route_started; // Flag indicating if route following has started
static int m_point_last; // Index pointing behind last point on the route (circular buffer)
static int m_point_now; // Index of the first point in the currently considered part of the route
static bool m_has_prev_point; // Flag indicating if there is a valid previous point
static float m_override_speed; // Manual speed override value (m/s)
static bool m_is_speed_override; // Flag indicating if speed override is active
static ROUTE_POINT m_rp_now; // The point in space we are currently following
static float m_rad_now; // Current turning radius (meters)
static ROUTE_POINT m_point_rx_prev; // Previous received point (for duplicate detection)
static bool m_point_rx_prev_set; // Flag indicating if previous point has been set
static mutex_t m_ap_lock; // Mutex for thread-safe access to autopilot data
static int32_t m_start_time; // Start time of current route (milliseconds since midnight)
static bool m_sync_rx; // Flag for synchronization state
static int m_print_closest_point; // Debug: print closest point every N iterations (0 = disabled)
static bool m_en_dynamic_rad; // Enable dynamic radius calculation based on speed
static bool m_en_angle_dist_comp; // Enable angle-distance compensation for steering
#ifdef IS_ROBANT
// Sidofel mot linjen, integrerat (rad styrvinkel). Ren "pure pursuit" lämnar ett
// stående sidofel när styrningen har dödband eller inte står exakt rakt: på mk1
// körde roboten ca 1 m bredvid linjen (2026-10-16). Den här delen drar in den.
#define AP_SIDA_KI			0.03	// rad per (m * s)
#define AP_SIDA_MAX			0.10	// rad (ca 6 grader)
#define AP_SIDA_MAX_FEL		1.5		// m; större fel (påväg in, skarp kurva) integreras inte
static float m_sida_i = 0.0;
static systime_t m_sida_t = 0;
#endif
static int m_route_look_ahead; // Number of points to look ahead along the route
static int m_route_left; // Number of points remaining in the route
static bool m_route_end; // Flag indicating if end of route has been reached

// Front wheel angle feedback (used for both differential steering and electric steering with feedback)
float m_turn_rad_now = 1e6; // Initialize to large value indicating no valid reading
// Current turning radius from front wheel feedback (1e6 = invalid/no reading)

extern int iDebug; // External debug variable for conditional debugging output
//extern float i_term; // Commented out: integral term for PID controller (not currently used)

// Private functions
static THD_FUNCTION(ap_thread, arg); // Main autopilot thread function
static void steering_angle_to_point(
		float current_x,
		float current_y,
		float current_angle,
		float goal_x,
		float goal_y,
		float *steering_angle,
		float *distance,
		float *circle_radius); // Calculate steering angle to reach a target point
static bool add_point(ROUTE_POINT *p, bool first); // Add a point to the route (internal)
static void clear_route(void); // Clear the current route (internal)
static void terminal_state(int argc, const char **argv); // Terminal command: print autopilot state
static void terminal_print_closest(int argc, const char **argv); // Terminal command: enable/disable closest point printing
static void terminal_dynamic_rad(int argc, const char **argv); // Terminal command: enable/disable dynamic radius
static void terminal_angle_dist_comp(int argc, const char **argv); // Terminal command: enable/disable angle-distance compensation
static void terminal_look_ahead(int argc, const char **argv); // Terminal command: set look-ahead distance
static void reset_state(void); // Reset autopilot state variables (internal)

// Initialize the autopilot module
// This function initializes all autopilot state variables, registers terminal commands,
// and starts the autopilot thread.
void autopilot_init(void) {
	// Clear route storage
	memset(m_route, 0, sizeof(m_route));
	
	// Initialize state flags
	m_is_active = false;
	m_is_route_started = false;
	
	// Initialize route indices
	m_point_now = 0;
	m_point_last = 0;
	m_has_prev_point = false;
	
	// Initialize speed override
	m_override_speed = 0.0;
	m_is_speed_override = false;
	
	// Initialize current point and radius
	memset(&m_rp_now, 0, sizeof(ROUTE_POINT));
	m_rad_now = -1.0; // -1.0 indicates no valid radius
	
	// Initialize previous point tracking
	memset(&m_point_rx_prev, 0, sizeof(ROUTE_POINT));
	m_point_rx_prev_set = false;
	
	// Initialize mutex for thread safety
	chMtxObjectInit(&m_ap_lock);
	
	// Initialize timing and synchronization
	m_start_time = 0;
	m_sync_rx = false;
	
	// Initialize debug and configuration settings
	m_print_closest_point = false; // Debug output disabled by default
	m_en_dynamic_rad = true; // Dynamic radius enabled by default
	m_en_angle_dist_comp = true; // Angle-distance compensation enabled by default
	m_route_look_ahead = 8; // Look ahead 8 points by default
	m_route_left = 0;
	m_route_end = false;

#if HAS_DIFF_STEERING
	m_turn_rad_now = 1e6;
#endif

	// Register terminal commands for autopilot control and debugging
	terminal_register_command_callback(
			"ap_state",
			"Print the state of the autopilot",
			"",
			terminal_state);

	terminal_register_command_callback(
			"ap_print_closest",
			"Print distance to closest route point.\n"
			"  0 - Disabled\n"
			"  n - Print closest point every n:th iteration.",
			"[print_rate]",
			terminal_print_closest);

	terminal_register_command_callback(
			"ap_dynamic_rad",
			"Enable or disable dynamic radius.\n"
			"  0 - Disabled\n"
			"  1 - Enabled",
			"[enabled]",
			terminal_dynamic_rad);

	terminal_register_command_callback(
			"ap_ang_dist_comp",
			"Enable or disable steering angle distance compensation.\n"
			"  0 - Disabled\n"
			"  1 - Enabled",
			"[enabled]",
			terminal_angle_dist_comp);

	terminal_register_command_callback(
			"ap_set_look_ahead",
			"Set the look-ahead distance along the route in points",
			"[points]",
			terminal_look_ahead);

	// Start the autopilot thread with normal priority
	chThdCreateStatic(ap_thread_wa, sizeof(ap_thread_wa),
			NORMALPRIO, ap_thread, NULL);
}

/**
 * Add a point to the route
 *
 * @param p
 * The point to add (ROUTE_POINT structure containing px, py, speed, time, attributes)
 *
 * @param first
 * True if this is the first point in the packet. Used to check for duplicate packets.
 *
 * @return
 * True if the point was added, false otherwise (e.g., if it's a duplicate).
 */
bool autopilot_add_point(ROUTE_POINT *p, bool first) {
	// Lock mutex for thread-safe access
	chMtxLock(&m_ap_lock);

	// Call internal add_point function
	bool res = add_point(p, first);

	// Unlock mutex
	chMtxUnlock(&m_ap_lock);

	return res;
}

// Remove the last point from the route
// This function removes the most recently added point from the route buffer.
// It ensures that we don't remove the current point being followed.
void autopilot_remove_last_point(void) {
	// Lock mutex for thread-safe access
	chMtxLock(&m_ap_lock);

	// Only remove if there are points to remove (m_point_last != m_point_now)
	if (m_point_last != m_point_now) {
		m_point_last--;
		// Handle circular buffer wrap-around
		if (m_point_last < 0) {
			m_point_last = AP_ROUTE_SIZE - 1;
		}
	}

	// Unlock mutex
	chMtxUnlock(&m_ap_lock);
}

// Clear the entire route
// This function clears all points from the route and resets the autopilot state.
void autopilot_clear_route(void) {
	// Lock mutex for thread-safe access
	chMtxLock(&m_ap_lock);

	// Call internal clear_route function
	clear_route();

	// Unlock mutex
	chMtxUnlock(&m_ap_lock);
}

// Replace the current route with a new point
// This function replaces the current route. If autopilot is inactive, it clears the route and adds the new point.
// If autopilot is active, it intelligently replaces the appropriate part of the route based on timestamps.
//
// @param p The new route point to add
// @return True if the point was added, false otherwise
bool autopilot_replace_route(ROUTE_POINT *p) {
	bool ret = false;

	// Lock mutex for thread-safe access
	chMtxLock(&m_ap_lock);

	if (!m_is_active) {
		// If autopilot is not active, clear the route and add the new point
		clear_route();
		add_point(p, true);
		ret = true;
	} else {
		// Check if we're using time-based routing (time > 0)
		bool time_mode = p->time > 0;

		// Find the appropriate position to insert the new point
		// Walk backwards through the route until we find the right spot
		while (m_point_last != m_point_now) {
			m_point_last--;
			// Handle circular buffer wrap-around
			if (m_point_last < 0) {
				m_point_last = AP_ROUTE_SIZE - 1;
			}

			// If we use time stamps (times are > 0), only overwrite the newer
			// part of the route.
			if (time_mode && p->time >= m_route[m_point_last].time) {
				break;
			}
		}

		// Update previous point flag
		m_has_prev_point = m_point_last != m_point_now;

		// In time mode, only add the point if its timestamp was ahead of the point
		// we currently follow.
		if (!time_mode || m_point_last != m_point_now || p->time >= m_route[m_point_last].time) {
			add_point(p, true);
			ret = true;
		}
	}

	// Unlock mutex
	chMtxUnlock(&m_ap_lock);

	return ret;
}

// Synchronize route points with timing information
// This function updates the speed of route points to ensure the vehicle reaches
// a specific point at a specific time. Used for time-synchronized route following.
//
// @param point Index of the target point to synchronize to
// @param time Time (in milliseconds) when the point should be reached
// @param min_time_diff Minimum time difference to consider synchronization valid
void autopilot_sync_point(int32_t point, int32_t time, int32_t min_time_diff) {
	// Lock mutex for thread-safe access
	chMtxLock(&m_ap_lock);

	// Commented out: Only update run when the autopilot is active
//	if (!m_is_active) {
//		chMtxUnlock(&m_ap_lock);
//		return;
//	}

	// Calculate start index (next point after current)
	int start = m_point_now + 1;
	if (start >= AP_ROUTE_SIZE) {
		start = 0;
	}

	// If start equals m_point_last, no points to process
	if (start == m_point_last) {
		chMtxUnlock(&m_ap_lock);
		return;
	}

	// Get current vehicle position
	POS_STATE p;

	if (main_config.vehicle.use_uwb_pos) {
		pos_uwb_get_pos(&p);
	} else {
		pos_get_pos(&p);
	}

	// vehicle center coordinates
	const float vehicle_cx = p.px;
	const float vehicle_cy = p.py;
	ROUTE_POINT vehicle_pos;
	vehicle_pos.px = vehicle_cx;
	vehicle_pos.py = vehicle_cy;

	// Calculate total distance from current position to target point
	int point_i = start;
	int point_prev = 0;
	float dist_tot = 0.0;

	// Calculate remaining length to target point
	for (;;) {
		if (point_i == start) {
			// First iteration: distance from vehicle to first route point
			dist_tot += utils_rp_distance(&vehicle_pos, &m_route[point_i]);
		} else {
			// Subsequent iterations: distance between route points
			dist_tot += utils_rp_distance(&m_route[point_prev], &m_route[point_i]);
		}

		// Stop when we reach the point before m_point_last or the target point
		if (point_i == (m_point_last - 1) || point_i == point) {
			break;
		}

		point_prev = point_i;
		point_i++;
		// Handle circular buffer wrap-around
		if (point_i >= AP_ROUTE_SIZE) {
			point_i = 0;
		}
	}

	// Skip synchronization if time or distance is too small
	if (time < min_time_diff || dist_tot < main_config.ap_base_rad) {
//		m_sync_rx = false;
		chMtxUnlock(&m_ap_lock);
		return;
	}

	// Calculate required speed to reach target point at specified time
	float speed = dist_tot / ((float)time / 1000.0);
	// Limit speed to maximum configured speed
	utils_truncate_number_abs(&speed, main_config.ap_max_speed);

	// Update speed for all points from current position to target point
	point_i = m_point_now;
	while (point_i <= point && point_i != m_point_last) {
		m_route[point_i].speed = speed;

		point_i++;
		// Handle circular buffer wrap-around
		if (point_i >= AP_ROUTE_SIZE) {
			point_i = 0;
		}
	}

	// Set synchronization flag
	m_sync_rx = true;

	// Unlock mutex
	chMtxUnlock(&m_ap_lock);
}

// Set the autopilot active state
// This function activates or deactivates the autopilot.
// When activating, it records the start time. When deactivating, it stops route following.
//
// @param active True to activate autopilot, false to deactivate
void autopilot_set_active(bool active) {
	// Lock mutex for thread-safe access
	chMtxLock(&m_ap_lock);

	// If activating and was previously inactive, record start time
	if (active && !m_is_active) {
		m_start_time = pos_get_ms_today();
#ifdef IS_ROBANT
		m_sida_i = 0.0;
#endif
//		m_sync_rx = false; // Commented out: reset sync flag on activation
	}

	// Stängs autopiloten av mitt i en rutt: stoppa gas och armar direkt (bara vid
	// övergången, timeout-tråden anropar detta var 10:e ms så länge förbindelsen saknas).
	if (!active && m_is_active) {
		motor_stop();
	}

	// Set active state
	m_is_active = active;

	// If route end was reached and we're activating, reset to first point
	if (m_route_end && m_is_active) {
		m_point_now = 0;
	}

	// Unlock mutex
	chMtxUnlock(&m_ap_lock);
}

// Check if autopilot is currently active
// @return True if autopilot is active, false otherwise
bool autopilot_is_active(void) {
	return m_is_active;
}

// Reset the autopilot state
// This function resets the autopilot state variables to their initial values.
void autopilot_reset_state(void) {
	// Lock mutex for thread-safe access
	chMtxLock(&m_ap_lock);

	// Call internal reset_state function
	reset_state();

	// Unlock mutex
	chMtxUnlock(&m_ap_lock);
}

// Internal function to reset autopilot state variables
static void reset_state(void) {
	m_point_now = 0;
	m_is_route_started = false;
	m_start_time = pos_get_ms_today();
	m_sync_rx = false;
	m_route_left = 0;
	m_route_end = false;
//	i_term=0; // Commented out: reset integral term
	memset(&m_rp_now, 0, sizeof(ROUTE_POINT));
}

// Get the current route length (number of points)
// @return Number of points in the current route
int autopilot_get_route_len(void) {
	return m_point_last;
}

// Get the index of the current point being followed
// @return Index of the current point
int autopilot_get_point_now(void) {
	return m_point_now;
}

// Get the number of points remaining in the route
// @return Number of points left to follow
int autopilot_get_route_left(void) {
	return m_route_left;
}

// Get a specific route point by index
// @param ind Index of the route point to retrieve
// @return The ROUTE_POINT at the specified index, or an empty point if index is invalid
ROUTE_POINT autopilot_get_route_point(int ind) {
	ROUTE_POINT res;
	memset(&res, 0, sizeof(ROUTE_POINT));

	// Only return valid point if index is within bounds
	if (ind < m_point_last) {
		res = m_route[ind];
	}

	return res;
}

/**
 * Override the speed with a fixed speed instead of using the value defined by
 * the route.
 *
 * @param is_override
 * True for override, false for using the route speed.
 *
 * @param speed
 * The speed to use (in m/s). Ignored if is_override is false.
 */
void autopilot_set_speed_override(bool is_override, float speed) {
	// Set override flag and speed value
	m_is_speed_override = is_override;
	m_override_speed = speed;
	// Note: This is not thread-safe as it doesn't use the mutex.
	// This is likely intentional for performance reasons in time-critical code.
}

/**
 * Set the motor speed directly.
 *
 * @param speed
 * Speed in m/s.
 */
void autopilot_set_motor_speed(float speed) {
	// Only set motor speed if motor is not disabled in configuration
	if (!main_config.vehicle.disable_motor) {
		// Commented out: Special handling for DRANGEN vehicle configuration
/*#if IS_DRANGEN
		// TODO
		comm_can_lock_vesc();
		comm_can_set_vesc_id(VESC_STEERING_VESC_LEFT);
		motor_set_rpm((int)speed);
		comm_can_set_vesc_id(VESC_STEERING_VESC_RIGHT);
		motor_set_rpm((int)rpm_r);
		comm_can_unlock_vesc();
#endif */
		/*
*/
		// Set motor speed through motor control interface
		motor_set_speed_autopilot(speed);
	}
}

/**
 * Get steering scale factor based on the current speed.
 * This implements speed-dependent steering: at low speeds, full steering is allowed,
 * but at higher speeds, steering is reduced for stability.
 *
 * @return
 * Steering scale factor. 1.0 at low speed (0 m/s), decreasing as speed increases.
 * The factor is calculated as 1/(1 + speed*0.05)^2, so at 10 m/s, the factor is ~0.5.
 */
float autopilot_get_steering_scale(void) {
	// Calculate divisor based on current speed
	// Higher speed = larger divisor = smaller steering scale
	const float div = 1.0 + fabsf(pos_get_speed()) * 0.05;
	// Return inverse of square for non-linear scaling
	return 1.0 / (div * div);
}

/**
 * Get the current radius for calculating the next goal point.
 * This radius is used for path following calculations.
 *
 * @return
 * The radius in meters. -1.0 indicates no valid radius.
 */
float autopilot_get_rad_now(void) {
	return m_rad_now;
}

/**
 * Get the current goal point for the autopilot.
 * This is the point the autopilot is currently trying to reach.
 *
 * @param rp
 * Pointer to store the goal point to.
 */
void autopilot_get_goal_now(ROUTE_POINT *rp) {
	*rp = m_rp_now;
}

#if HAS_DIFF_STEERING
// Set the turning radius from front wheel feedback (for differential steering)
// @param rad The turning radius in meters
void autopilot_set_turn_rad(float rad) {
	m_turn_rad_now = rad;
}
#endif

// Main autopilot thread function
// This thread runs at AP_HZ (100Hz) and performs the following tasks:
// 1. Checks if autopilot is active
// 2. Gets current vehicle position
// 3. Handles attribute-based control (positioning, hydraulics)
// 4. Calculates path following using circle-line intersection
// 5. Computes steering angle and speed
// 6. Sends commands to motor control
static THD_FUNCTION(ap_thread, arg) {
	(void)arg; // Unused parameter

	// Set thread name for debugging
	chRegSetThreadName("Autopilot");

	// Current attributes state (for attribute-based control)
	uint32_t attributes_now = 0;

	// Main thread loop - runs forever
	for(;;) {
		// Commented out: Emergency stop event handling
/*        if (chEvtWaitOneTimeout(EMERGENCY_STOP_EVENT, TIME_IMMEDIATE) == MSG_OK) {
            // Perform cleanup if necessary
            // ...
            break;
        }*/

		// Sleep to maintain AP_HZ frequency (100Hz = 10ms period)
		chThdSleep(CH_CFG_ST_FREQUENCY / AP_HZ);

		// Lock mutex for thread-safe access to autopilot data
		chMtxLock(&m_ap_lock);

		// If autopilot is not active, set invalid radius and continue
		if (!m_is_active) {
			m_rad_now = -1.0;
			chMtxUnlock(&m_ap_lock);
			continue;
		}

		// Calculate the length of the route
		int len = m_point_last;

		// This means that the route has wrapped around (circular buffer)
		// (should only happen when ap_repeat_routes == false)
		if (m_point_now > m_point_last) {
			len = AP_ROUTE_SIZE + m_point_last - m_point_now;
		}

		// Calculate the length of the route that is left to follow
		m_route_left = len - m_point_now;
		if (m_route_left < 0) {
			m_route_left += AP_ROUTE_SIZE;
		}

		// Get current time of day according to our clock (milliseconds since midnight)
		int ms_today = pos_get_ms_today();

		if (len >= 2) {
			POS_STATE pos_now;

            // Set positioning according to attributes
			switch (attributes_now & ATTR_POSITIONING_MASK) {
			case ATTR_POSITIONING_DEFAULT:
				pos_get_pos(&pos_now);
				break;

 			case ATTR_POSITIONING_UWB:
				pos_uwb_get_pos(&pos_now);
				break;

			default:
				if (main_config.vehicle.use_uwb_pos) {
					pos_uwb_get_pos(&pos_now);
				} else {
					pos_get_pos(&pos_now);
				}
				break;
            }

#if HAS_HYDRAULIC_DRIVE
            // Hydraulic move according to attributes
            // NOTE: currently, only one valve can be actuated at a time using attributes
            // Debug output for attribute values
			if (iDebug==76)
			{
				commands_printf("Attribute: %ui",attributes_now);
			}

            // Handle hydraulic control based on attribute flags
            switch (attributes_now & ATTR_HYDRAULIC_MASK) {
            case ATTR_HYDRAULIC_FRONT_UP:
    			if (iDebug==77)
    			{
    				commands_printf("Front up");
    			}
                hydraulic_move(HYDRAULIC_POS_FRONT, HYDRAULIC_MOVE_UP);
                hydraulic_move(HYDRAULIC_POS_REAR, HYDRAULIC_MOVE_STOP);
                hydraulic_move(HYDRAULIC_POS_EXTRA, HYDRAULIC_MOVE_STOP);
                break;
            case ATTR_HYDRAULIC_FRONT_DOWN:
    			if (iDebug==77)
    			{
    				commands_printf("Front down");
    			}
                hydraulic_move(HYDRAULIC_POS_FRONT, HYDRAULIC_MOVE_DOWN);
                hydraulic_move(HYDRAULIC_POS_REAR, HYDRAULIC_MOVE_STOP);
                hydraulic_move(HYDRAULIC_POS_EXTRA, HYDRAULIC_MOVE_STOP);
                break;
            case ATTR_HYDRAULIC_REAR_UP:
                hydraulic_move(HYDRAULIC_POS_FRONT, HYDRAULIC_MOVE_STOP);
                hydraulic_move(HYDRAULIC_POS_REAR, HYDRAULIC_MOVE_UP);
                hydraulic_move(HYDRAULIC_POS_EXTRA, HYDRAULIC_MOVE_STOP);
                break;
            case ATTR_HYDRAULIC_REAR_DOWN:
                hydraulic_move(HYDRAULIC_POS_FRONT, HYDRAULIC_MOVE_STOP);
                hydraulic_move(HYDRAULIC_POS_REAR, HYDRAULIC_MOVE_DOWN);
                hydraulic_move(HYDRAULIC_POS_EXTRA, HYDRAULIC_MOVE_STOP);
                break;
            case ATTR_HYDRAULIC_EXTRA_OUT:
                hydraulic_move(HYDRAULIC_POS_FRONT, HYDRAULIC_MOVE_STOP);
                hydraulic_move(HYDRAULIC_POS_REAR, HYDRAULIC_MOVE_STOP);
                hydraulic_move(HYDRAULIC_POS_EXTRA, HYDRAULIC_MOVE_OUT);
                break;
            case ATTR_HYDRAULIC_EXTRA_IN:
                hydraulic_move(HYDRAULIC_POS_FRONT, HYDRAULIC_MOVE_STOP);
                hydraulic_move(HYDRAULIC_POS_REAR, HYDRAULIC_MOVE_STOP);
                hydraulic_move(HYDRAULIC_POS_EXTRA, HYDRAULIC_MOVE_IN);
                break;
            default:
                hydraulic_move(HYDRAULIC_POS_FRONT, HYDRAULIC_MOVE_STOP);
                hydraulic_move(HYDRAULIC_POS_REAR, HYDRAULIC_MOVE_STOP);
                hydraulic_move(HYDRAULIC_POS_EXTRA, HYDRAULIC_MOVE_STOP);
                break;
            }
#endif

			// vehicle center
			const float vehicle_cx = pos_now.px;
			const float vehicle_cy = pos_now.py;
			ROUTE_POINT vehicle_pos;
			vehicle_pos.px = vehicle_cx;
			vehicle_pos.py = vehicle_cy;

			// Speed-dependent radius
			if (m_en_dynamic_rad) {
				m_rad_now = fabsf(main_config.ap_rad_time_ahead * pos_get_speed());
				if (m_rad_now < main_config.ap_base_rad) {
					m_rad_now = main_config.ap_base_rad;
				}
			} else {
				m_rad_now = main_config.ap_base_rad;
			}

			// Look m_route_look_ahead points ahead, or less than that if the route is shorter
			int look_ahead = m_route_look_ahead;
			if (look_ahead >= len) {
				look_ahead = len-1;
			}

			int start;
			int end;
			ROUTE_POINT rp_now; // The point we should follow now.
			int circle_intersections = 0;
			bool last_point_reached = false;

			if (m_is_route_started) {
				start = m_point_now;
				end = m_point_now + look_ahead;
			} else { // initially, go to first point of the route
				start = 0;
				end = 0;
				rp_now = m_route[0];
				circle_intersections = 1;

				if (utils_rp_distance(&rp_now, &vehicle_pos) < 1.5*m_rad_now) // leave a bit more slack than turn radius, might come from a very bad angle
					m_is_route_started = true;
			}

			// Last point in route
			int last_point_ind = m_point_last - 1;
			if (last_point_ind < 0) {
				last_point_ind += AP_ROUTE_SIZE;
			}

			ROUTE_POINT *rp_last = &m_route[last_point_ind]; // Last point on route
			ROUTE_POINT *rp_ls1 = &m_route[0]; // First point on goal line segment
			ROUTE_POINT *rp_ls2 = &m_route[1]; // Second point on goal line segment

			ROUTE_POINT *closest1_speed = &m_route[0];
			ROUTE_POINT *closest2_speed = &m_route[1];

			ROUTE_POINT *closest_to_vehicle = &m_route[0];
/*			if ((iDebug==23))
			{
				commands_printf("closest_to_vehicle, x: %f, y: %f",closest_to_vehicle->px,closest_to_vehicle->py);
			}*/

			for (int i = start;i < end;i++) {
				int ind = i; // First point index for this iteration
				int indn = i + 1; // Next point index for this iteration

				// Wrap around
				if (ind >= m_point_last) {
					if (m_point_now <= m_point_last) {
						ind -= m_point_last;
					} else {
						if (ind >= AP_ROUTE_SIZE) {
							ind -= AP_ROUTE_SIZE;
						}
					}
				}

				// Wrap around
				if (indn >= m_point_last) {
					if (m_point_now <= m_point_last) {
						indn -= m_point_last;
					} else {
						if (indn >= AP_ROUTE_SIZE) {
							indn -= AP_ROUTE_SIZE;
						}
					}
				}

				// Find closest point to vehicle
				if (utils_rp_distance(&vehicle_pos, &m_route[ind]) <
						utils_rp_distance(&vehicle_pos, closest_to_vehicle)) {
					closest_to_vehicle = &m_route[ind];
				}

				// Check for circle intersection. If there are many intersections
				// found in this loop, the last one will be used.
				ROUTE_POINT int1, int2;
				ROUTE_POINT *p1, *p2;
				p1 = &m_route[ind];
				p2 = &m_route[indn];

				// If the next point has a time before the current point and repeat route is
				// active we have completed a full route. Increase its time by the repetition time.
				if (main_config.ap_repeat_routes && utils_time_before(p2->time, p1->time)) {
					p2->time += main_config.ap_time_add_repeat_ms;
					if (p2->time > MS_PER_DAY) {
						p2->time -= MS_PER_DAY;
					}
				}

				int res = utils_circle_line_int(vehicle_cx, vehicle_cy, m_rad_now, p1, p2, &int1, &int2);

				if (res) {
					closest1_speed = p1;
					closest2_speed = p2;
				}

				// One intersection. Use it.
				if (res == 1) {
					circle_intersections++;
					rp_now = int1;
				}

				// Two intersections. Use the point with the most "progress" on the route.
				if (res == 2) {
					circle_intersections += 2;

					if (utils_rp_distance(&int1, p2) < utils_rp_distance(&int2, p2)) {
						rp_now = int1;
					} else {
						rp_now = int2;
					}
				}

				if (res > 0) {
					rp_ls1 = &m_route[ind];
					rp_ls2 = &m_route[indn];
				}

				// If we aren't repeating routes and there is an intersection on the last
				// line segment, go straight to the last point.
				if (!main_config.ap_repeat_routes) {
					if (indn == last_point_ind && circle_intersections > 0) {
						if (res > 0) {
							last_point_reached = true;
						}
						break;
					}
				}
			}

			attributes_now = m_is_route_started ? closest_to_vehicle->attributes : 0;

			// Look for closest points
			ROUTE_POINT closest; // Closest point on route to vehicle
			ROUTE_POINT *closest1 = &m_route[0]; // Start of closest line segment
			ROUTE_POINT *closest2 = &m_route[1]; // End of closest line segment
			int closest1_ind = 0; // Index of the first closest point

			bool closest_set = false;

			for (int i = start;i < end;i++) {
				int ind = i; // First point index for this iteration
				int indn = i + 1; // Next point index for this iteration

				// Wrap around
				if (ind >= m_point_last) {
					if (m_point_now <= m_point_last) {
						ind -= m_point_last;
					} else {
						if (ind >= AP_ROUTE_SIZE) {
							ind -= AP_ROUTE_SIZE;
						}
					}
				}

				// Wrap around
				if (indn >= m_point_last) {
					if (m_point_now <= m_point_last) {
						indn -= m_point_last;
					} else {
						if (indn >= AP_ROUTE_SIZE) {
							indn -= AP_ROUTE_SIZE;
						}
					}
				}

				ROUTE_POINT tmp;
				ROUTE_POINT *p1, *p2;
				p1 = &m_route[ind];
				p2 = &m_route[indn];
				utils_closest_point_line(p1, p2, vehicle_cx, vehicle_cy, &tmp);

				if (!closest_set || utils_rp_distance(&tmp, &vehicle_pos) < utils_rp_distance(&closest, &vehicle_pos)) {
					closest_set = true;
					closest = tmp;
					closest1 = p1;
					closest2 = p2;
					closest1_ind = ind;
				}

				// Do not look past the last point if we aren't repeating routes.
				if (!main_config.ap_repeat_routes) {
					if (indn == last_point_ind) {
						break;
					}
				}
			}

			if (circle_intersections == 0) {
				closest1_speed = closest1;
				closest2_speed = closest2;
			}

			static int sample = 0;
			static int print_before = 0;
			if (m_print_closest_point) {
				if (print_before == 0) {
					sample = 0;
				}

				if (sample % m_print_closest_point == 0) {
					float diff = utils_rp_distance(&closest, &vehicle_pos) * 100.0;
					float speed = pos_now.speed * 3.6;

					commands_printf("D: %.1f cm, S: %.2f km/h, Yaw: %.1f deg, Rad: %.2f m, closest1_ind: %d",
							(double)diff, (double)speed, (double)pos_now.yaw, (double)m_rad_now, closest1_ind);
				}

				sample++;
			}
			print_before = m_print_closest_point;

			if (last_point_reached) {
				rp_now = *rp_last;
			} else {
				// Use the closest point on the considered route if no
				// circle intersection is found.
				if (circle_intersections == 0) {
					rp_now = closest;
					rp_ls1 = closest1;
					rp_ls2 = closest2;
				}
			}

			// Check if the end of route is reached
			if (!main_config.ap_repeat_routes && m_route_left < 3 &&
					utils_rp_distance(&m_route[last_point_ind], &vehicle_pos) < m_rad_now) {
				m_route_end = true;
			} else {
				m_route_end = false;
			}

			m_point_now = closest1_ind;
			m_rp_now = rp_now;

			if (!m_route_end) {
				float distance = 0.0;
				float steering_angle = 0.0;
				float circle_radius = 1000.0;

				//debugvalue8=-pos_now.yaw * M_PI / 180.0;

				steering_angle_to_point(pos_now.px, pos_now.py, -pos_now.yaw * M_PI / 180.0, rp_now.px,
						rp_now.py, &steering_angle, &distance, &circle_radius);

#ifdef IS_ROBANT
				{
					float dt = ST2MS(chVTTimeElapsedSinceX(m_sida_t)) / 1000.0;
					m_sida_t = chVTGetSystemTimeX();
					if (dt > 0.2) {
						dt = 0.2;
					}
					// Sidofel: + = fordonet till vänster om linjen (closest1 -> closest2).
					float sx = closest2->px - closest1->px;
					float sy = closest2->py - closest1->py;
					float slen = sqrtf(sx * sx + sy * sy);
					if (closest_set && m_is_route_started && slen > 0.05 && pos_now.speed > 0.1) {
						float sida = (sx * (vehicle_cy - closest.py) - sy * (vehicle_cx - closest.px)) / slen;
						if (fabsf(sida) < AP_SIDA_MAX_FEL) {
							m_sida_i += AP_SIDA_KI * sida * dt;
							utils_truncate_number_abs(&m_sida_i, AP_SIDA_MAX);
						}
					}
					// + styrvinkel = sväng vänster; till vänster om linjen ska den svänga höger.
					steering_angle -= m_sida_i;
				}
#endif
				//debugvalue7=distance;
				//debugvalue11=pos_now.px;
				//debugvalue12=pos_now.py;

#if !HAS_DIFF_STEERING
				// Scale maximum steering by speed
				float max_rad = main_config.vehicle.steering_max_angle_rad * autopilot_get_steering_scale();

				if (steering_angle >= max_rad) {
					steering_angle = max_rad;
				} else if (steering_angle <= -max_rad) {
					steering_angle = -max_rad;
				}

				float servo_pos = steering_angle
						/ ((2.0 * main_config.vehicle.steering_max_angle_rad)
								/ main_config.vehicle.steering_range)
								+ main_config.vehicle.steering_center;
				//debugvalue9=steering_angle;
				//debugvalue10=servo_pos;
#endif

				float speed = 0.0;

				if (main_config.ap_mode_time && !m_sync_rx) {
					if (ms_today >= 0) {
						// Calculate speed such that the route points are reached at their
						// specified time. Notice that the direct distance between the vehicle
						// and the points is used and not the arc that the vehicle drives. This
						// should still work well enough.

						int32_t dist_prev = (int32_t)(utils_rp_distance(&rp_now, rp_ls1) * 1000.0);
						int32_t dist_tot = (int32_t)(utils_rp_distance(&rp_now, rp_ls1) * 1000.0);
						dist_tot += (int32_t)(utils_rp_distance(&rp_now, rp_ls2) * 1000.0);
						int32_t time = utils_map_int(dist_prev, 0, dist_tot, rp_ls1->time, rp_ls2->time);
						float dist_vehicle = utils_rp_distance(&vehicle_pos, &rp_now);

						int32_t t_diff = time - ms_today;

						if (main_config.ap_mode_time == 2) {
							t_diff += m_start_time;
						}

						if (t_diff < 0) {
							t_diff += 24 * 60 * 60 * 1000;
						}

						if (t_diff > 0) {
							speed = dist_vehicle / ((float)t_diff / 1000.0);
						} else {
							speed = 0.0;
						}
					} else {
						speed = 0.0;
					}
				} else {
					// Calculate the speed based on the average speed between the two closest points
					const float dist_prev = utils_rp_distance(&rp_now, closest1_speed);
					const float dist_tot = utils_rp_distance(&rp_now, closest1_speed) + utils_rp_distance(&rp_now, closest2_speed);
					speed = utils_map(dist_prev, 0.0, dist_tot, closest1_speed->speed, closest2_speed->speed);
				}

				if (m_is_speed_override) {
					speed = m_override_speed;
				}

				utils_truncate_number_abs(&speed, main_config.ap_max_speed);

				// All vehicle-specific control moved to motor_control
				motor_set_steering_autopilot(steering_angle, circle_radius);
				motor_set_speed_autopilot(speed);
			}
		}

		if (m_route_end) {
			motor_handle_route_end();
			m_rad_now = -1.0;
			m_is_active = false;
			reset_state();
		}

		chMtxUnlock(&m_ap_lock);
	}
}

static void steering_angle_to_point(
		float current_x,
		float current_y,
		float current_angle,
		float goal_x,
		float goal_y,
		float *steering_angle,
		float *distance,
		float *circle_radius) {

	const float D = utils_point_distance(goal_x, goal_y, current_x, current_y);
	*distance = D;
	const float gamma = current_angle - atan2f((goal_y-current_y), (goal_x-current_x));
	const float dx = D * cosf(gamma);
	const float dy = D * sinf(gamma);

	if (fabsf(dy) <= 0.000001) {
		*steering_angle = 0.0;
		*circle_radius = 9000.0;
		return;
	}

	float R = -(dx * dx + dy * dy) / (2.0 * dy);

	/*
	 * Add correction if the arc is much longer than the total distance.
	 * TODO: Find a good model.
	 */
	float angle_correction = 1.0 + (m_en_angle_dist_comp ? D * 0.2 : 0.0);
	if (angle_correction > 5.0) {
		angle_correction = 5.0;
	}

	R /= angle_correction;
//	commands_printf("angle_correction: %f\n", angle_correction);
	//	commands_printf("R (2): %f\n", R);

	*circle_radius = R;
	*steering_angle = atanf(main_config.vehicle.axis_distance / R);
}

static bool add_point(ROUTE_POINT *p, bool first) {
	if (first && m_point_rx_prev_set &&
			utils_point_distance(m_point_rx_prev.px, m_point_rx_prev.py, p->px, p->py) < 1e-4) {
		return false;
	}

	if (first) {
		m_point_rx_prev = *p;
		m_point_rx_prev_set = true;
	}

	m_route[m_point_last++] = *p;

	if (m_point_last >= AP_ROUTE_SIZE) {
		m_point_last = 0;
	}

	// Make sure that there always is a valid point when looking backwards in the route
	if (!m_has_prev_point) {
		int p_last = m_point_now - 1;
		if (p_last < 0) {
			p_last += AP_ROUTE_SIZE;
		}

		m_route[p_last] = *p;
		m_has_prev_point = true;
	}

	// When repeating routes, the previous point for the first
	// point is the end point of the current route.
	if (main_config.ap_repeat_routes) {
		m_route[AP_ROUTE_SIZE - 1] = *p;
	}

	return true;
}

static void clear_route(void) {
	m_is_active = false;
	m_has_prev_point = false;
	m_point_now = 0;
	m_point_last = 0;
	m_point_rx_prev_set = false;
	m_start_time = pos_get_ms_today();
	m_sync_rx = false;
	memset(&m_rp_now, 0, sizeof(ROUTE_POINT));
	memset(&m_point_rx_prev, 0, sizeof(ROUTE_POINT));
}

static void terminal_state(int argc, const char **argv) {
	(void)argc;
	(void)argv;

	commands_printf(
			"m_is_active: %i\n"
			"m_has_prev_point: %i\n"
			"m_point_now: %i\n"
			"m_point_last: %i\n"
			"m_point_rx_prev_set: %i\n"
			"m_start_time: %i\n"
			"m_route_left: %i\n"
			"m_route_end: %i\n",

			m_is_active,
			m_has_prev_point,
			m_point_now,
			m_point_last,
			m_point_rx_prev_set,
			m_start_time,
			m_route_left,
			m_route_end);
}

static void terminal_print_closest(int argc, const char **argv) {
	if (argc == 2) {
		int n = -1;
		sscanf(argv[1], "%d", &n);

		if (n < 0) {
			commands_printf("Invalid argument\n");
		} else {
			if (n > 0) {
				commands_printf("OK. Printing closest point every %d iterations.\n", n);
			} else {
				commands_printf("OK. Not printing closest point.\n", n);
			}

			m_print_closest_point = n;
		}
	} else {
		commands_printf("Wrong number of arguments\n");
	}
}

static void terminal_dynamic_rad(int argc, const char **argv) {
	if (argc == 2) {
		if (strcmp(argv[1], "0") == 0) {
			m_en_dynamic_rad = 0;
			commands_printf("OK\n");
		} else if (strcmp(argv[1], "1") == 0) {
			m_en_dynamic_rad = 1;
			commands_printf("OK\n");
		} else {
			commands_printf("Invalid argument %s\n", argv[1]);
		}
	} else {
		commands_printf("Wrong number of arguments\n");
	}
}

static void terminal_angle_dist_comp(int argc, const char **argv) {
	if (argc == 2) {
		if (strcmp(argv[1], "0") == 0) {
			m_en_angle_dist_comp = 0;
			commands_printf("Angle distance compensation disabled\n");
		} else if (strcmp(argv[1], "1") == 0) {
			m_en_angle_dist_comp = 1;
			commands_printf("Angle distance compensation enabled\n");
		} else {
			commands_printf("Invalid argument %s\n", argv[1]);
		}
	} else {
		commands_printf("Wrong number of arguments\n");
	}
}

static void terminal_look_ahead(int argc, const char **argv) {
	if (argc == 2) {
		int n = -1;
		sscanf(argv[1], "%d", &n);

		if (n < 1) {
			commands_printf("Invalid argument\n");
		} else {
			m_route_look_ahead = n;
			commands_printf("Now looking %d points ahead along the route", n);
		}
	} else {
		commands_printf("Wrong number of arguments\n");
	}
}
