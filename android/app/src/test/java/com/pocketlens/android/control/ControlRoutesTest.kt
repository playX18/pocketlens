package com.pocketlens.android.control

import kotlin.test.Test
import kotlin.test.assertEquals

class ControlRoutesTest {
    @Test
    fun routesMatchSharedPlan() {
        assertEquals("/status", ControlRoutes.STATUS)
        assertEquals("/pair", ControlRoutes.PAIR)
        assertEquals("/session/start", ControlRoutes.SESSION_START)
        assertEquals("/session/stop", ControlRoutes.SESSION_STOP)
        assertEquals("/session/events", ControlRoutes.SESSION_EVENTS)
    }
}
