package dev.lorehaven.songbook

import android.content.Context
import android.graphics.Bitmap
import android.graphics.Color
import android.graphics.Typeface
import android.graphics.drawable.GradientDrawable
import android.net.Uri
import android.util.LruCache
import android.view.Gravity
import android.view.View
import android.widget.ImageView
import android.widget.LinearLayout
import android.widget.TextView
import androidx.media3.datasource.DataSourceBitmapLoader
import java.util.concurrent.Executors

/** Colours and small builders of the phone screens (plain views, no layout files). */
object Ui {
    val BACK = Color.parseColor("#0b0b0c")
    val PANEL = Color.parseColor("#151517")
    val RAISED = Color.parseColor("#1f1f23")
    val TEXT = Color.parseColor("#f2f2f3")
    val DIM = Color.parseColor("#a2a2ab")
    val ACCENT = Color.parseColor("#1ed760")
    val DANGER = Color.parseColor("#ff6b6b")

    /** The screens are drawn edge to edge (Android 15 and up): keep what matters out from under the status and navigation bars. */
    fun fitSystemBars(view: View) {
        view.setOnApplyWindowInsetsListener { v, insets ->
            @Suppress("DEPRECATION")
            v.setPadding(insets.systemWindowInsetLeft, insets.systemWindowInsetTop, insets.systemWindowInsetRight, insets.systemWindowInsetBottom)
            insets
        }
        view.requestApplyInsets()
    }

    fun dp(context: Context, value: Int) = (value * context.resources.displayMetrics.density).toInt()

    fun text(context: Context, text: String, size: Float = 15f, color: Int = TEXT, bold: Boolean = false) = TextView(context).apply {
        this.text = text
        textSize = size
        setTextColor(color)
        if (bold) setTypeface(typeface, Typeface.BOLD)
    }

    /** A button made of text: `primary` is the green pill, otherwise a grey one. */
    fun button(context: Context, label: String, primary: Boolean = false, click: () -> Unit) = TextView(context).apply {
        text = label
        textSize = 14f
        gravity = Gravity.CENTER
        setTextColor(if (primary) Color.parseColor("#06210f") else TEXT)
        setTypeface(typeface, Typeface.BOLD)
        setPadding(dp(context, 16), dp(context, 10), dp(context, 16), dp(context, 10))
        background = GradientDrawable().apply { cornerRadius = dp(context, 24).toFloat(); setColor(if (primary) ACCENT else RAISED) }
        isClickable = true
        setOnClickListener { click() }
    }

    fun row(context: Context) = LinearLayout(context).apply {
        orientation = LinearLayout.HORIZONTAL
        gravity = Gravity.CENTER_VERTICAL
    }

    fun column(context: Context) = LinearLayout(context).apply { orientation = LinearLayout.VERTICAL }

    fun weight(w: Float = 1f) = LinearLayout.LayoutParams(0, LinearLayout.LayoutParams.WRAP_CONTENT, w)

    fun square(context: Context, size: Int) = LinearLayout.LayoutParams(dp(context, size), dp(context, size))
}

/** Pictures for the lists: read through the cache that also feeds the car (so what was seen shows without the network). */
class Images(context: Context) {
    private val loader = DataSourceBitmapLoader.Builder(context).setDataSourceFactory(Offline.of(context).pictures).build()
    private val memory = object : LruCache<String, Bitmap>(24 * 1024 * 1024) {
        override fun sizeOf(key: String, value: Bitmap) = value.byteCount
    }
    private val executor = Executors.newFixedThreadPool(2)

    /** Puts the picture at `uri` in `view` when it comes; the tile behind it shows meanwhile (and when there is none). */
    fun load(uri: Uri?, view: ImageView) {
        view.tag = uri?.toString()
        view.setImageDrawable(null)
        val key = uri?.toString() ?: return
        memory.get(key)?.let { view.setImageBitmap(it); return }
        executor.execute {
            try {
                val bitmap = loader.loadBitmap(uri).get()
                memory.put(key, bitmap)
                view.post { if (view.tag == key) view.setImageBitmap(bitmap) }
            } catch (e: Exception) {
                // No picture: the tile stays.
            }
        }
    }

    /** A square tile for a cover: a dark tile with the picture over it. */
    fun cover(context: Context, size: Int): ImageView = ImageView(context).apply {
        scaleType = ImageView.ScaleType.CENTER_CROP
        background = GradientDrawable().apply { cornerRadius = Ui.dp(context, 6).toFloat(); setColor(Ui.RAISED) }
        clipToOutline = true
        layoutParams = Ui.square(context, size)
    }
}

fun View.show(on: Boolean) {
    visibility = if (on) View.VISIBLE else View.GONE
}
