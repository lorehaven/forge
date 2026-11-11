package dev.lorehaven.daybook

import android.app.Activity
import android.app.AlertDialog
import android.app.DatePickerDialog
import android.app.TimePickerDialog
import android.os.Bundle
import android.view.Gravity
import android.view.ViewGroup
import android.view.WindowManager
import android.view.inputmethod.EditorInfo
import android.widget.Button
import android.widget.EditText
import android.widget.LinearLayout
import android.widget.Toast
import java.time.LocalDate
import java.time.LocalTime
import java.time.format.DateTimeFormatter

/**
 * The widget's **+**: a small dialog over the home screen to make a task without opening the app. A title (a date written in
 * it, like "call mum tomorrow 5pm", is understood by the program), buttons for the date, the time, the reminder and the list,
 * and *Add*. The work is done by the app's program in the background ([QuickAdd]); the app never opens.
 */
class QuickAddActivity : Activity() {
    private var date: LocalDate? = null
    private var time: LocalTime? = null
    private var remind: Preset? = null
    private var listId = WidgetData.INBOX

    private lateinit var title: EditText
    private lateinit var dateButton: Button
    private lateinit var timeButton: Button
    private lateinit var remindButton: Button
    private lateinit var listButton: Button

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        val snapshot = WidgetData.snapshot(this)
        if (snapshot == null) {
            // Nobody is signed in: say so, and stay out of the way (no app opens).
            Toast.makeText(this, R.string.widget_empty_signed_out, Toast.LENGTH_LONG).show()
            finish()
            return
        }
        listId = WidgetData.effectiveSelection(snapshot, intent.getStringExtra(EXTRA_LIST) ?: WidgetData.INBOX)
        window.setSoftInputMode(WindowManager.LayoutParams.SOFT_INPUT_STATE_VISIBLE or WindowManager.LayoutParams.SOFT_INPUT_ADJUST_RESIZE)
        window.setGravity(Gravity.BOTTOM or Gravity.CENTER_HORIZONTAL)

        title = EditText(this).apply {
            hint = getString(R.string.quick_add_hint)
            setSingleLine(true)
            imeOptions = EditorInfo.IME_ACTION_DONE
            setOnEditorActionListener { _, action, _ -> (action == EditorInfo.IME_ACTION_DONE).also { if (it) submit() } }
        }
        dateButton = chip { pickDate() }
        timeButton = chip { pickTime() }
        remindButton = chip { pickReminder() }
        listButton = chip { pickList(snapshot) }
        // One compact bar over the keyboard: the title, then the chips and the send button on a line.
        val chips = LinearLayout(this).apply {
            orientation = LinearLayout.HORIZONTAL
            addView(dateButton)
            addView(timeButton)
            addView(remindButton)
            addView(listButton)
        }
        val scroller = android.widget.HorizontalScrollView(this).apply {
            isHorizontalScrollBarEnabled = false
            addView(chips)
        }
        val send = Button(this, null, android.R.attr.borderlessButtonStyle).apply {
            setText(R.string.quick_add_add)
            setOnClickListener { submit() }
        }
        val line = LinearLayout(this).apply {
            orientation = LinearLayout.HORIZONTAL
            gravity = Gravity.CENTER_VERTICAL
            addView(scroller, LinearLayout.LayoutParams(0, ViewGroup.LayoutParams.WRAP_CONTENT, 1f))
            addView(send)
        }
        val root = LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
            setPadding(dp(16), dp(12), dp(8), dp(4))
            addView(title)
            addView(line)
        }
        setContentView(root)
        // The theme is translucent (nothing of its own is drawn), so the card is drawn here.
        window.setBackgroundDrawable(android.graphics.drawable.GradientDrawable().apply {
            setColor(0xFF2A2A30.toInt())
            val r = dp(16).toFloat()
            cornerRadii = floatArrayOf(r, r, r, r, 0f, 0f, 0f, 0f)
        })
        window.setLayout(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT)
        refresh(snapshot)
        title.requestFocus()
        // Asked for outright: the window flag alone is not always honoured over a launcher.
        title.post { getSystemService(android.view.inputmethod.InputMethodManager::class.java)?.showSoftInput(title, 0) }
    }

    private fun chip(onClick: () -> Unit) = Button(this, null, android.R.attr.buttonBarButtonStyle).apply {
        isAllCaps = false
        maxLines = 1
        minWidth = 0
        minimumWidth = 0
        setOnClickListener { onClick() }
    }

    private fun weighted() = LinearLayout.LayoutParams(0, ViewGroup.LayoutParams.WRAP_CONTENT, 1f)

    private fun dp(value: Int) = (value * resources.displayMetrics.density).toInt()

    private fun refresh(snapshot: Snapshot? = WidgetData.snapshot(this)) {
        dateButton.text = date?.format(DateTimeFormatter.ofPattern("EEE d MMM")) ?: getString(R.string.quick_add_date)
        timeButton.text = time?.format(DateTimeFormatter.ofPattern("HH:mm")) ?: getString(R.string.quick_add_time)
        timeButton.isEnabled = date != null
        remindButton.text = remind?.label ?: getString(R.string.quick_add_remind)
        remindButton.isEnabled = date != null
        listButton.text = snapshot?.lists?.firstOrNull { it.id == listId }?.name ?: getString(R.string.quick_add_inbox)
    }

    private fun pickDate() {
        val start = date ?: LocalDate.now()
        DatePickerDialog(this, { _, y, m, d ->
            date = LocalDate.of(y, m + 1, d)
            normalizeReminder()
            refresh()
        }, start.year, start.monthValue - 1, start.dayOfMonth).apply {
            setButton(AlertDialog.BUTTON_NEUTRAL, getString(R.string.quick_add_clear)) { _, _ ->
                date = null
                time = null
                remind = null
                refresh()
            }
        }.show()
    }

    private fun pickTime() {
        val start = time ?: LocalTime.of(9, 0)
        TimePickerDialog(this, { _, h, m ->
            time = LocalTime.of(h, m)
            normalizeReminder()
            refresh()
        }, start.hour, start.minute, android.text.format.DateFormat.is24HourFormat(this)).apply {
            setButton(AlertDialog.BUTTON_NEUTRAL, getString(R.string.quick_add_all_day)) { _, _ ->
                time = null
                normalizeReminder()
                refresh()
            }
        }.show()
    }

    /** A reminder follows the due date, and the choices differ for a day with a time and one without: drop one that no longer fits. */
    private fun normalizeReminder() {
        if (remind?.allDay != (time == null)) remind = null
    }

    private fun pickReminder() {
        val options = Preset.values().filter { it.allDay == (time == null) }
        val names = (listOf(getString(R.string.quick_add_no_reminder)) + options.map { it.label }).toTypedArray()
        AlertDialog.Builder(this)
            .setTitle(R.string.quick_add_remind)
            .setItems(names) { _, which ->
                remind = if (which == 0) null else options[which - 1]
                refresh()
            }
            .show()
    }

    private fun pickList(snapshot: Snapshot) {
        val lists = snapshot.lists
        val names = lists.map { if (it.kind == "label") "# ${it.name}" else it.name }.toTypedArray()
        AlertDialog.Builder(this)
            .setTitle(R.string.widget_pick_list)
            .setSingleChoiceItems(names, lists.indexOfFirst { it.id == listId }) { dialog, which ->
                listId = lists[which].id
                refresh(snapshot)
                dialog.dismiss()
            }
            .show()
    }

    private fun submit() {
        val text = title.text.toString().trim()
        if (text.isEmpty()) {
            title.error = getString(R.string.quick_add_need_title)
            return
        }
        val due = date?.let { d -> time?.let { "${d}T${it.format(DateTimeFormatter.ofPattern("HH:mm"))}" } ?: d.toString() }
        QuickAdd.start(this, text, listId, due, remind?.key)
        Toast.makeText(this, R.string.quick_add_adding, Toast.LENGTH_SHORT).show()
        finish()
    }

    /** The reminder presets (the same words as the app's), by the key the program takes. */
    private enum class Preset(val key: String, val label: String, val allDay: Boolean) {
        AT_DUE("at_due", "At due time", false),
        TEN_MINUTES("10m", "10 minutes before", false),
        ONE_HOUR("1h", "1 hour before", false),
        ONE_DAY("1d", "1 day before", false),
        ON_THE_DAY("on_the_day", "On the day, 09:00", true),
        DAY_BEFORE("day_before", "The day before, 09:00", true),
        TWO_DAYS_BEFORE("2d_before", "2 days before, 09:00", true),
    }

    companion object {
        const val EXTRA_LIST = "list"
    }
}
