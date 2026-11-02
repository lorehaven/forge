package dev.lorehaven.songbook

import android.content.Context
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import java.io.File
import java.security.KeyStore
import java.security.SecureRandom
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec

/**
 * The key the native program seals its session file with. A random 32-byte data key is kept in `session_key`,
 * encrypted by an Android Keystore key that cannot be exported from the device: a copy of the app's files (a
 * backup, a pulled data directory) holds only the session file and a key nobody can open. The program gets the data
 * key as a hex string in `FORGE_SESSION_KEY`, in its environment only.
 *
 * Without a working Keystore the answer is null and the program keeps its file as plain text, as before: being
 * signed in matters more than the extra protection.
 */
object SessionKey {
    private const val ALIAS = "forge-session-wrap"
    private const val FILE = "session_key"

    @Synchronized
    fun hex(context: Context): String? = try {
        val file = File(context.filesDir, FILE)
        val key = if (file.exists()) {
            unwrap(file.readBytes())
        } else {
            ByteArray(32).also { SecureRandom().nextBytes(it) }.also { fresh ->
                val tmp = File(context.filesDir, "$FILE.tmp")
                tmp.writeBytes(wrap(fresh))
                tmp.renameTo(file)
            }
        }
        key.joinToString("") { "%02x".format(it) }
    } catch (e: Exception) {
        null
    }

    private fun wrapKey(): SecretKey {
        val store = KeyStore.getInstance("AndroidKeyStore").apply { load(null) }
        (store.getKey(ALIAS, null) as? SecretKey)?.let { return it }
        return KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES, "AndroidKeyStore").run {
            init(
                KeyGenParameterSpec.Builder(ALIAS, KeyProperties.PURPOSE_ENCRYPT or KeyProperties.PURPOSE_DECRYPT)
                    .setBlockModes(KeyProperties.BLOCK_MODE_GCM)
                    .setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE)
                    .setKeySize(256)
                    .build(),
            )
            generateKey()
        }
    }

    /** `iv || ciphertext`. */
    private fun wrap(data: ByteArray): ByteArray {
        val cipher = Cipher.getInstance("AES/GCM/NoPadding").apply { init(Cipher.ENCRYPT_MODE, wrapKey()) }
        return cipher.iv + cipher.doFinal(data)
    }

    private fun unwrap(blob: ByteArray): ByteArray {
        val cipher = Cipher.getInstance("AES/GCM/NoPadding").apply {
            init(Cipher.DECRYPT_MODE, wrapKey(), GCMParameterSpec(128, blob.copyOfRange(0, 12)))
        }
        return cipher.doFinal(blob, 12, blob.size - 12)
    }
}
