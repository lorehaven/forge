package dev.lorehaven.songbook

import android.Manifest
import android.app.Activity
import android.content.Intent
import android.content.pm.PackageManager
import android.graphics.Color
import android.net.Uri
import android.os.Build
import android.os.Bundle
import android.text.InputType
import android.view.Gravity
import android.view.View
import android.window.OnBackInvokedDispatcher
import android.widget.Button
import android.widget.EditText
import android.widget.LinearLayout
import android.widget.TextView
import android.widget.ScrollView

/**
 * The one screen of the phone: sign in (the car cannot ask for a password) and see who is signed in. What is played, and
 * where it is chosen, is the media session: Android Auto, the lock screen and the notification.
 */
class MainActivity : Activity() {
    private lateinit var session: Session
    private var phone: Phone? = null
    private val backdrop = Color.parseColor("#0b0b0c")

    override fun onCreate(state: Bundle?) {
        super.onCreate(state)
        session = Session.of(this)
        if (Build.VERSION.SDK_INT >= 33 && checkSelfPermission(Manifest.permission.POST_NOTIFICATIONS) != PackageManager.PERMISSION_GRANTED) {
            requestPermissions(arrayOf(Manifest.permission.POST_NOTIFICATIONS), 1)
        }
        // Back: from Android 13 on (and always for an app made for 16) it is a callback, not `onBackPressed`.
        if (Build.VERSION.SDK_INT >= 33) {
            onBackInvokedDispatcher.registerOnBackInvokedCallback(OnBackInvokedDispatcher.PRIORITY_DEFAULT) {
                if (phone?.back() != true) finish()
            }
        }
        // A debug build can be signed in from the command line (`am start -e server ... -e user ... -e password ...`).
        if (BuildConfig.DEBUG && intent.hasExtra("server")) signInFrom(intent) else show()
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        if (BuildConfig.DEBUG && intent.hasExtra("server")) signInFrom(intent)
    }

    private fun signInFrom(intent: Intent) {
        signInInBackground(intent.getStringExtra("server") ?: "", intent.getStringExtra("user") ?: "", intent.getStringExtra("password") ?: "")
    }

    override fun onStop() {
        super.onStop()
        phone?.stop()
        phone = null
    }

    override fun onStart() {
        super.onStart()
        if (session.signedIn && phone == null) show()
    }

    @Deprecated("Deprecated in Java")
    override fun onBackPressed() {
        // Before Android 13 (from then on the callback above is what is called).
        if (Build.VERSION.SDK_INT < 33 && phone?.back() != true) super.onBackPressed()
    }

    private fun text(text: String, size: Float = 16f, bold: Boolean = false) = Ui.text(this, text, size, bold = bold).apply { setPadding(0, 12, 0, 12) }

    private fun field(hint: String, value: String = "", password: Boolean = false) = EditText(this).apply {
        this.hint = hint
        setText(value)
        setTextColor(Color.WHITE)
        setHintTextColor(Color.GRAY)
        inputType = if (password) InputType.TYPE_CLASS_TEXT or InputType.TYPE_TEXT_VARIATION_PASSWORD else InputType.TYPE_CLASS_TEXT or InputType.TYPE_TEXT_VARIATION_URI
        setSingleLine()
    }

    private fun show(error: String? = null) {
        phone?.stop()
        phone = null
        if (session.signedIn) {
            val p = Phone(this) { show() }
            phone = p
            setContentView(p.root)
            Ui.fitSystemBars(p.root)
            p.start()
        } else {
            setContentView(signIn(error))
        }
    }

    private fun signIn(error: String? = null): View {
        val root = LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
            setPadding(48, 96, 48, 48)
            setBackgroundColor(backdrop)
        }
        val server = field("Server", session.server.ifEmpty { "https://" })
        val user = field("User name")
        val password = field("Password", password = true)
        val message = text(error ?: "").apply { setTextColor(Color.parseColor("#ff6b6b")); visibility = if (error == null) View.GONE else View.VISIBLE }
        val go = Ui.button(this, "Sign in", primary = true) {}
        go.setOnClickListener {
            go.isEnabled = false
            message.visibility = View.GONE
            signInInBackground(server.text.toString(), user.text.toString(), password.text.toString())
        }
        root.addView(text("Songbook", 28f, true))
        listOf<View>(text("Sign in with your account on the server that runs Songbook."), server, user, password, message, go).forEach { root.addView(it) }
        return ScrollView(this).apply { setBackgroundColor(backdrop); addView(root); Ui.fitSystemBars(this) }
    }

    private fun signInInBackground(server: String, user: String, password: String) {
        Thread {
            val why = try {
                session.login(server, user, password)
                null
            } catch (e: IllegalArgumentException) {
                e.message ?: "Could not sign in."
            } catch (e: Unreachable) {
                "Cannot reach the server: ${e.message}"
            }
            runOnUiThread {
                show(why)
            }
        }.start()
    }
}
