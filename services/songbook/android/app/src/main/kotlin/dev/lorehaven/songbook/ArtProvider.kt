package dev.lorehaven.songbook

import android.content.ContentProvider
import android.content.ContentValues
import android.database.Cursor
import android.net.Uri
import android.os.ParcelFileDescriptor
import android.util.Base64
import androidx.media3.datasource.DataSpec
import java.io.File
import java.io.FileNotFoundException

/**
 * Pictures for Android Auto. The car's host fetches the artwork of the browse lists itself and has no token, so a service address
 * gets it nothing. The lists give `content://<authority>/<address>` instead; this reads the picture through the app's cached,
 * signed layer ([Offline.pictures]) and hands the file over.
 */
class ArtProvider : ContentProvider() {
    override fun onCreate() = true

    override fun openFile(uri: Uri, mode: String): ParcelFileDescriptor {
        val context = context ?: throw FileNotFoundException()
        val address = original(uri) ?: throw FileNotFoundException(uri.toString())
        val file = File(File(context.cacheDir, "art-out").apply { mkdirs() }, Integer.toHexString(address.hashCode()) + ".img")
        if (!file.exists() || file.length() == 0L) {
            val tmp = File(file.path + ".tmp")
            try {
                val source = Offline.of(context).pictures.createDataSource()
                try {
                    source.open(DataSpec(Uri.parse(address)))
                    tmp.outputStream().use { out ->
                        val buffer = ByteArray(16 * 1024)
                        while (true) {
                            val n = source.read(buffer, 0, buffer.size)
                            if (n < 0) break
                            out.write(buffer, 0, n)
                        }
                    }
                } finally {
                    source.close()
                }
                tmp.renameTo(file)
            } catch (e: Exception) {
                tmp.delete()
                throw FileNotFoundException(e.message)
            }
        }
        return ParcelFileDescriptor.open(file, ParcelFileDescriptor.MODE_READ_ONLY)
    }

    override fun getType(uri: Uri): String? = null
    override fun query(uri: Uri, projection: Array<String>?, selection: String?, selectionArgs: Array<String>?, sortOrder: String?): Cursor? = null
    override fun insert(uri: Uri, values: ContentValues?): Uri? = null
    override fun delete(uri: Uri, selection: String?, selectionArgs: Array<String>?) = 0
    override fun update(uri: Uri, values: ContentValues?, selection: String?, selectionArgs: Array<String>?) = 0

    companion object {
        const val AUTHORITY = "dev.lorehaven.songbook.art"

        /** The address the car can read for a picture at a service `address`. */
        fun uri(address: String): Uri =
            Uri.Builder().scheme("content").authority(AUTHORITY)
                .appendPath(Base64.encodeToString(address.toByteArray(), Base64.URL_SAFE or Base64.NO_WRAP or Base64.NO_PADDING)).build()

        /** The service address behind [uri] (null when it is not one of ours). */
        fun original(uri: Uri): String? {
            if (uri.scheme != "content" || uri.authority != AUTHORITY) return null
            val part = uri.lastPathSegment ?: return null
            return try { String(Base64.decode(part, Base64.URL_SAFE or Base64.NO_WRAP or Base64.NO_PADDING)) } catch (e: IllegalArgumentException) { null }
        }
    }
}
