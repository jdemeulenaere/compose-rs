plugins {
    id("com.android.application")
}

// The benchmark's compose-rs app: examples/feed in a ComposeRsView, in a plain Activity (no
// AndroidX but androidx.profileinstaller, as bench-compose), the same screens as bench-compose's.
android {
    namespace = "rs.compose.bench.rs"
    compileSdk = 36
    // Pinned (README.md): the Android Gradle plugin strips native libraries only when it's set.
    ndkVersion = "27.2.12479018"
    defaultConfig {
        applicationId = "rs.compose.bench.rs"
        minSdk = 29
        targetSdk = 36
        versionCode = 1
        versionName = "0.1"
    }
    buildTypes {
        release {
            isMinifyEnabled = true
            proguardFiles(getDefaultProguardFile("proguard-android-optimize.txt"), "proguard-rules.pro")
            signingConfig = signingConfigs.getByName("debug")
        }
    }
    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }
}

// Builds the feed's Rust library (examples/feed) for arm64 with build-rust.sh.
// Each variant's jniLibs take the task's output directory, so Gradle runs it before merging the
// native libraries and packages the library it built (an Exec task without declared outputs,
// run before preBuild, sometimes left the previous build's library in the APK).
// Never up to date: cargo decides what to rebuild.
abstract class BuildRust : Exec() {
    @get:OutputDirectory
    abstract val outputDirectory: DirectoryProperty
}

androidComponents {
    onVariants { variant ->
        val name = "buildRust" + variant.name.replaceFirstChar(Char::uppercase)
        // Cargo's Android profiles (Cargo.toml, D361): both abort on panics; the release one
        // adds fat LTO and one codegen unit, which debuggable builds skip to rebuild faster.
        // By the build type's debuggable flag, so other release-like types take LTO too.
        val debuggable = variant.buildType?.let { android.buildTypes.getByName(it).isDebuggable } ?: true
        val cargoProfile = if (debuggable) "android" else "android-release"
        val buildRust = tasks.register<BuildRust>(name) {
            val output = outputDirectory
            executable = rootDir.resolve("build-rust.sh").path
            args("feed", "compose_rs_feed")
            argumentProviders.add(CommandLineArgumentProvider { listOf(output.get().asFile.path, cargoProfile) })
            environment("ANDROID_NDK_VERSION", android.ndkVersion)
            // What build-rust.sh reads, through providers: the configuration cache keeps the
            // environment the task was configured with, and is discarded when these change.
            // Unset ones are passed empty (the script reads empty as unset), so each is always
            // the value the cache entry was made with.
            val variables = listOf("ANDROID_NDK_HOME", "ANDROID_HOME", "ANDROID_SDK_ROOT", "HOME", "PATH")
            for (variable in variables) {
                environment(variable, providers.environmentVariable(variable).getOrElse(""))
            }
            outputs.upToDateWhen { false }
        }
        variant.sources.jniLibs?.addGeneratedSourceDirectory(buildRust, BuildRust::outputDirectory)
    }
}

dependencies {
    implementation(project(":compose-rs"))
    // Hands the APK's Baseline Profile (compose-rs's, merged) to ART on installs that don't come
    // from Play, as bench-compose's does for Compose's (D360): measure.py --compile speed-profile.
    implementation("androidx.profileinstaller:profileinstaller:1.4.1")
    constraints {
        // Its dependencies at the versions bench-compose resolves through Compose's BOM, which
        // an offline build finds in Gradle's cache.
        implementation("androidx.annotation:annotation:1.9.1")
        implementation("androidx.tracing:tracing:1.2.0")
    }
}
