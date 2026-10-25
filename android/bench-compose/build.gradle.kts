plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.plugin.compose")
}

// The benchmark's Kotlin Compose app: the same screens as bench-rs's compose-rs one
// (examples/feed), built as Compose recommends for performance (minified, release).
android {
    namespace = "rs.compose.bench.compose"
    compileSdk = 37
    defaultConfig {
        applicationId = "rs.compose.bench.compose"
        minSdk = 29
        targetSdk = 37
        versionCode = 1
        versionName = "0.1"
    }
    buildFeatures {
        compose = true
    }
    buildTypes {
        release {
            isMinifyEnabled = true
            isShrinkResources = true
            proguardFiles(getDefaultProguardFile("proguard-android-optimize.txt"))
            signingConfig = signingConfigs.getByName("debug")
        }
    }
    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }
}

dependencies {
    implementation(platform("androidx.compose:compose-bom:2026.09.00"))
    implementation("androidx.compose.ui:ui")
    implementation("androidx.compose.material3:material3")
    implementation("androidx.compose.material:material-icons-core")
    implementation("androidx.activity:activity-compose:1.13.0")
    implementation("androidx.profileinstaller:profileinstaller:1.4.1")
}
