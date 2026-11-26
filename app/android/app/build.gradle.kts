import java.util.Properties

plugins {
    id("com.android.application")
    // gradle.properties sets android.builtInKotlin=false, so apply Kotlin here, then the
    // Compose compiler (Glance widgets are Compose code and fail at runtime without it).
    id("org.jetbrains.kotlin.android")
    id("org.jetbrains.kotlin.plugin.compose")
    // The Flutter Gradle Plugin must be applied after the Android and Kotlin Gradle plugins.
    id("dev.flutter.flutter-gradle-plugin")
}

// Release signing (never committed): app/android/key.properties, see plan/07.
val releaseKey = Properties().apply {
    val f = rootProject.file("key.properties")
    if (f.exists()) f.inputStream().use { load(it) }
}

// Google Calendar on the phone (plan/19 slice 5). The Android OAuth client id belongs to the
// user's Google Cloud project, so it never goes in the repo: set utre.googleAndroidClientId
// in ~/.gradle/gradle.properties or app/android/google.properties (git-ignored), pass
// -Putre.googleAndroidClientId=..., or export UTRE_GOOGLE_ANDROID_CLIENT_ID. Google matches
// the client by package name and signing key, so the id is only the switch that turns the
// Google page on; without it the page says what is missing.
val googleProps = Properties().apply {
    val f = rootProject.file("google.properties")
    if (f.exists()) f.inputStream().use { load(it) }
}
val googleAndroidClientId: String =
    (findProperty("utre.googleAndroidClientId") as String?)
        ?: googleProps.getProperty("utre.googleAndroidClientId")
        ?: System.getenv("UTRE_GOOGLE_ANDROID_CLIENT_ID")
        ?: ""

android {
    namespace = "dev.utre.utre"
    compileSdk = flutter.compileSdkVersion
    ndkVersion = flutter.ndkVersion

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    defaultConfig {
        // TODO: Specify your own unique Application ID (https://developer.android.com/studio/build/application-id.html).
        applicationId = "dev.utre.utre"
        // You can update the following values to match your application needs.
        // For more information, see: https://flutter.dev/to/review-gradle-config.
        minSdk = flutter.minSdkVersion
        targetSdk = flutter.targetSdkVersion
        // Uses the version code from pubspec.yaml. When using split APKs, 1000 * ABI_VERSION
        // is added automatically by Flutter. (https://developer.android.com/studio/build/configure-apk-splits#configure-APK-versions)
        // You can force using the value of versionCode by specifying the `-P force-version-code-ignoring-abi=true`
        // flag during build.
        versionCode = flutter.versionCode
        versionName = flutter.versionName
        testInstrumentationRunner = "androidx.test.runner.AndroidJUnitRunner"
        manifestPlaceholders["googleAndroidClientId"] = googleAndroidClientId
    }

    signingConfigs {
        if (!releaseKey.isEmpty) {
            create("release") {
                storeFile = file(releaseKey.getProperty("storeFile"))
                storePassword = releaseKey.getProperty("storePassword")
                keyAlias = releaseKey.getProperty("keyAlias")
                keyPassword = releaseKey.getProperty("keyPassword")
            }
        }
    }

    buildTypes {
        release {
            // The release key when app/android/key.properties names one (plan/07 "Release
            // signing"; CI writes it from secrets), else the debug key so local builds work.
            signingConfig = if (releaseKey.isEmpty) signingConfigs.getByName("debug")
                else signingConfigs.getByName("release")
            // Keep JNA and the UniFFI bindings, which R8 can't see being used through reflection.
            proguardFiles(getDefaultProguardFile("proguard-android-optimize.txt"), "proguard-rules.pro")
        }
    }
}

kotlin {
    compilerOptions {
        jvmTarget = org.jetbrains.kotlin.gradle.dsl.JvmTarget.JVM_17
    }
}

flutter {
    source = "../.."
}

dependencies {
    // Home-screen widget
    implementation("androidx.glance:glance-appwidget:1.2.0")
    implementation("androidx.glance:glance-material3:1.2.0")
    implementation("org.jetbrains.kotlinx:kotlinx-coroutines-android:1.11.0")

    // Background sync (M4)
    implementation("androidx.work:work-runtime-ktx:2.12.0")

    // Google Calendar (M5b): Google Identity's AuthorizationClient gives the phone access tokens
    implementation("com.google.android.gms:play-services-auth:22.0.0")

    // UniFFI's Kotlin bindings call the Rust core through JNA
    implementation("net.java.dev.jna:jna:5.19.1@aar")

    // The test runner itself comes from Flutter's integration_test plugin (it pins androidx.test:runner).
    androidTestImplementation("androidx.test.ext:junit:1.3.0")
}
