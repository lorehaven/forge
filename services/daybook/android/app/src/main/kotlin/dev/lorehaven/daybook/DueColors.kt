package dev.lorehaven.daybook

import java.time.Instant
import java.time.LocalDate
import java.time.LocalTime
import java.time.ZoneId
import java.time.ZoneOffset
import java.time.temporal.ChronoUnit

/**
 * A due date as the widget shows it: text and colour, worked out *when the widget draws*, so a snapshot that
 * is hours old still says "Tomorrow" correctly after midnight.
 *
 * Mirrors `daybook_core::calendar` (`urgency`, `format_due`) - change them together.
 */
object DueColors {
    const val OVERDUE = 0xFFF43F5E.toInt() // brighter than the app's error red, which is dim on this background
    const val TODAY = 0xFF4ADE80.toInt() // green
    const val TOMORROW = 0xFFFB923C.toInt() // orange
    const val SOON = 0xFFFACC15.toInt() // yellow: within 3 days
    const val THIS_WEEK = 0xFF60A5FA.toInt() // blue: within a week
    const val LATER = 0xFFD4D4D4.toInt() // plain, beyond a week

    private val WEEKDAYS = arrayOf("Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun")
    private val MONTHS = arrayOf("Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec")

    data class Rendered(val text: String, val color: Int)

    /**
     * [dueMillis] is the due instant. With [allDay] it is read as the UTC date it stores, not converted to the
     * device's zone, and shown without a time.
     */
    fun render(
        dueMillis: Long,
        allDay: Boolean,
        nowMillis: Long,
        zone: ZoneId = ZoneId.systemDefault(),
    ): Rendered {
        val today: LocalDate = Instant.ofEpochMilli(nowMillis).atZone(zone).toLocalDate()
        val date: LocalDate
        val time: LocalTime?
        if (allDay) {
            date = Instant.ofEpochMilli(dueMillis).atZone(ZoneOffset.UTC).toLocalDate()
            time = null
        } else {
            val local = Instant.ofEpochMilli(dueMillis).atZone(zone)
            date = local.toLocalDate()
            time = local.toLocalTime()
        }

        val days = ChronoUnit.DAYS.between(today, date)
        // An all-day task is overdue once its day has passed; a timed one once its time has.
        val overdue = if (allDay) date.isBefore(today) else dueMillis < nowMillis

        val color = when {
            overdue -> OVERDUE
            days == 0L -> TODAY
            days == 1L -> TOMORROW
            days <= 3 -> SOON
            days <= 7 -> THIS_WEEK
            else -> LATER
        }

        val day = when (days) {
            0L -> "Today"
            1L -> "Tomorrow"
            -1L -> "Yesterday"
            else -> {
                val base = "${WEEKDAYS[date.dayOfWeek.value - 1]} ${date.dayOfMonth} ${MONTHS[date.monthValue - 1]}"
                if (date.year == today.year) base else "$base ${date.year}"
            }
        }
        val text = if (time == null) day else "$day ${"%02d".format(time.hour)}:${"%02d".format(time.minute)}"
        return Rendered(text, color)
    }
}
