plugins {
    id("com.android.library")
}

android {
    namespace = "rs.compose.android"
    compileSdk = 37
    defaultConfig {
        minSdk = 29
        // The methods Rust calls through JNI, kept in minified apps.
        consumerProguardFiles("consumer-rules.pro")
    }
    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }
}

dependencies {
    // Compile-only: an AndroidX activity's back dispatcher is used when the app has one
    // (Back.kt); the library itself needs no AndroidX.
    // Only the classes it names, not their dependencies.
    compileOnly("androidx.activity:activity:1.13.0") { isTransitive = false }
    compileOnly("androidx.lifecycle:lifecycle-common-jvm:2.9.4") { isTransitive = false }
}
