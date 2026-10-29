package dev.lorehaven.hoist

import android.content.res.AssetManager
import android.net.Uri
import android.webkit.WebResourceResponse
import java.io.FileNotFoundException

/**
 * Serves the web bundle from the app's assets (the contents of `dist/www`) as if it came from an https origin, so
 * cookies, storage and the content-security policy behave as they do on the web.
 */
class AppAssets(private val assets: AssetManager) {
    companion object {
        const val HOST = "app.quench.local"
        const val ORIGIN = "https://$HOST"

        /** The policy of the server's pages (no inline code), for the bundle's own origin. */
        const val POLICY = "default-src 'self'; script-src 'self' 'nonce-bundle' 'wasm-unsafe-eval'; " +
            "style-src 'self'; connect-src 'self'; img-src 'self' data:; object-src 'none'; " +
            "base-uri 'none'; form-action 'self'; frame-ancestors 'none'"

        private val TYPES = mapOf(
            "html" to "text/html",
            "js" to "text/javascript",
            "mjs" to "text/javascript",
            "css" to "text/css",
            "wasm" to "application/wasm",
            "json" to "application/json",
            "svg" to "image/svg+xml",
            "png" to "image/png",
            "jpg" to "image/jpeg",
            "jpeg" to "image/jpeg",
            "ico" to "image/x-icon",
            "woff2" to "font/woff2",
            "txt" to "text/plain",
        )

        fun mimeOf(path: String): String =
            TYPES[path.substringAfterLast('.', "").lowercase()] ?: "application/octet-stream"
    }

    /** True when [url] is on the bundle's origin. */
    fun owns(url: Uri): Boolean = url.scheme == "https" && url.host == HOST

    /** The response for a request to the bundle's origin: a file, the page for a route, or a 404. */
    fun serve(url: Uri): WebResourceResponse {
        val path = url.path ?: "/"
        if (path.split('/').any { it == ".." }) return missing()
        // A path without an extension is a route of the app: the page, which the router takes over.
        val file = if (path == "/" || !path.substringAfterLast('/').contains('.')) "/index.html" else path
        return try {
            val stream = assets.open(file.removePrefix("/"))
            val html = file.endsWith(".html")
            val headers = mutableMapOf(
                "Cache-Control" to if (file.startsWith("/_quench/")) "public, max-age=31536000, immutable" else "no-cache",
                "X-Content-Type-Options" to "nosniff",
            )
            if (html) {
                headers["Content-Security-Policy"] = POLICY
                headers["Referrer-Policy"] = "same-origin"
            }
            WebResourceResponse(mimeOf(file), if (html) "utf-8" else null, 200, "OK", headers, stream)
        } catch (_: FileNotFoundException) {
            missing()
        }
    }

    private fun missing() = WebResourceResponse(
        "text/plain", "utf-8", 404, "Not Found", mapOf("Cache-Control" to "no-store"),
        "not found".byteInputStream(),
    )
}