package dev.lorehaven.klaxon

import android.app.Activity
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.IntentFilter
import android.graphics.Color
import android.graphics.Typeface
import android.graphics.drawable.GradientDrawable
import android.os.Bundle
import android.view.Gravity
import android.view.WindowManager
import android.widget.Button
import android.widget.LinearLayout
import android.widget.TextClock
import android.widget.TextView

/**
 * The screen of a ringing alarm: over the lock screen, with the screen turned on, the time, the label and two big
 * buttons. It does nothing itself; the buttons tell [RingService], and it closes when the ringing ends.
 */
class RingActivity : Activity() {
    private val ended = object : BroadcastReceiver() {
        override fun onReceive(context: Context, intent: Intent) {
            finish()
        }
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setShowWhenLocked(true)
        setTurnScreenOn(true)
        window.addFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON)
        val label = intent.getStringExtra("label") ?: ""
        val snooze = intent.getIntExtra("snooze", 10)

        val dp = resources.displayMetrics.density
        fun px(v: Int) = (v * dp).toInt()
        fun button(text: String, fill: Int, onClick: () -> Unit) = Button(this).apply {
            this.text = text
            textSize = 20f
            isAllCaps = false
            setTextColor(Color.WHITE)
            background = GradientDrawable().apply { setColor(fill); cornerRadius = px(32).toFloat() }
            layoutParams = LinearLayout.LayoutParams(LinearLayout.LayoutParams.MATCH_PARENT, px(72)).apply { topMargin = px(16) }
            setOnClickListener { onClick() }
        }

        val root = LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
            gravity = Gravity.CENTER_HORIZONTAL
            setBackgroundColor(Color.parseColor("#0b0b0b"))
            setPadding(px(32), px(96), px(32), px(48))
        }
        root.addView(TextClock(this).apply {
            format24Hour = "HH:mm"
            format12Hour = "HH:mm"
            textSize = 88f
            setTextColor(Color.parseColor("#e5e5e5"))
            typeface = Typeface.create("sans-serif-light", Typeface.NORMAL)
            gravity = Gravity.CENTER
        })
        root.addView(TextView(this).apply {
            text = label.ifEmpty { "Alarm" }
            textSize = 24f
            setTextColor(Color.parseColor("#a3a3a3"))
            gravity = Gravity.CENTER
        })
        // The buttons sit at the bottom, where a thumb is.
        root.addView(android.view.View(this), LinearLayout.LayoutParams(0, 0, 1f))
        root.addView(button("Snooze $snooze min", Color.parseColor("#374151")) { RingService.send(this, RingService.SNOOZE) }.apply { contentDescription = "Snooze" })
        root.addView(button("Dismiss", Color.parseColor("#0f766e")) { RingService.send(this, RingService.DISMISS) }.apply { contentDescription = "Dismiss" })
        setContentView(root)
        registerReceiver(ended, IntentFilter(RingService.ENDED), RECEIVER_NOT_EXPORTED)
    }

    // Back does not silence an alarm: it would be too easy to do asleep.
    @Deprecated("the platform's back handling")
    override fun onBackPressed() {}

    override fun onDestroy() {
        unregisterReceiver(ended)
        super.onDestroy()
    }
}
