package dev.lorehaven.daybook

import android.app.Activity
import android.content.Intent
import android.os.Bundle

/**
 * Where a tap on a widget row goes: no screen of its own. The row's text opens the app on the task (`daybook://open?...`, handed
 * on to [SurfaceActivity]), and its check mark marks the task done (`daybook://complete?task=...`) without opening anything.
 */
class WidgetActionActivity : Activity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        val link = intent?.data
        if (link?.scheme == "daybook" && link.host == "complete") {
            link.getQueryParameter("task")?.let { Complete.start(this, it, 0) }
        } else if (link != null) {
            startActivity(Intent(this, SurfaceActivity::class.java).setData(link).addFlags(Intent.FLAG_ACTIVITY_SINGLE_TOP))
        }
        finish()
    }
}
