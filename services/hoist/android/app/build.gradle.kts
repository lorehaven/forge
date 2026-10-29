plugins {
    id("com.android.application")
}

android {
    namespace = "dev.lorehaven.hoist"
    compileSdk = 36

    defaultConfig {
        applicationId = "dev.lorehaven.hoist"
        minSdk = 26
        targetSdk = 36
        // `quench build apk` passes the version from quench.toml.
        versionCode = (project.findProperty("quench.versionCode") as String?)?.toInt() ?: 1
        versionName = (project.findProperty("quench.versionName") as String?) ?: "0.1.0"
    }

    // aapt skips asset directories that start with an underscore by default, which would drop the
    // bundle's `_quench/` (the hashed assets). Everything else of its default list stays.
    androidResources {
        ignoreAssetsPatterns += "!.svn:!.git:!.ds_store:!*.scc:.*:!CVS:!thumbs.db:!picasa.ini:!*~"
    }

    // The native program (libquench_native.so) is run from the app's native library directory, so
    // the libraries are extracted at install time instead of being mapped from the APK.
    packaging {
        jniLibs {
            useLegacyPackaging = true
        }
    }

    buildFeatures {
        buildConfig = true
    }

    // The release key comes from the environment (`quench` sets the keystore and alias from
    // quench.toml; the passwords are only ever in the environment, never in a file).
    signingConfigs {
        create("release") {
            val keystore = System.getenv("QUENCH_KEYSTORE")
            if (keystore != null) {
                storeFile = file(keystore)
                storePassword = System.getenv("QUENCH_KEYSTORE_PASSWORD")
                keyAlias = System.getenv("QUENCH_KEY_ALIAS")
                keyPassword = System.getenv("QUENCH_KEY_PASSWORD") ?: System.getenv("QUENCH_KEYSTORE_PASSWORD")
            }
        }
    }

    buildTypes {
        release {
            isMinifyEnabled = false
            if (System.getenv("QUENCH_KEYSTORE") != null) signingConfig = signingConfigs.getByName("release")
        }
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    // The app is the web bundle `quench build --bundle` writes; the host only serves it.
    sourceSets {
        getByName("main") {
            assets.srcDir(file("../../dist/www"))
        }
    }
}

kotlin {
    compilerOptions {
        jvmTarget = org.jetbrains.kotlin.gradle.dsl.JvmTarget.JVM_17
    }
}