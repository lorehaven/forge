plugins {
    id("com.android.application")
}

android {
    namespace = "dev.lorehaven.songbook"
    compileSdk = 36

    defaultConfig {
        applicationId = "dev.lorehaven.songbook"
        minSdk = 26
        targetSdk = 36
        // `anvil artifact release` passes the version of the release (the same for every build of it).
        versionCode = (project.findProperty("quench.versionCode") as String?)?.toInt() ?: 1
        versionName = (project.findProperty("quench.versionName") as String?) ?: "0.1.0"
        testInstrumentationRunner = "androidx.test.runner.AndroidJUnitRunner"
    }

    buildFeatures {
        buildConfig = true
    }

    // The release key comes from the environment, as for the other apps: the passwords are never in a file.
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
}

kotlin {
    compilerOptions {
        jvmTarget = org.jetbrains.kotlin.gradle.dsl.JvmTarget.JVM_17
    }
}

dependencies {
    // The player and the session that Android Auto, the lock screen and the notification talk to.
    implementation("androidx.media3:media3-exoplayer:1.11.1")
    implementation("androidx.media3:media3-session:1.11.1")
    implementation("com.google.guava:guava:33.4.8-android")

    androidTestImplementation("androidx.test.ext:junit:1.2.1")
    androidTestImplementation("androidx.test:runner:1.6.2")
    androidTestImplementation("androidx.test.uiautomator:uiautomator:2.3.0")
}
