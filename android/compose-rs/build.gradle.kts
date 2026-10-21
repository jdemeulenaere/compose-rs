plugins {
    id("com.android.library")
}

android {
    namespace = "rs.compose.android"
    compileSdk = 36
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
