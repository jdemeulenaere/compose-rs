pluginManagement {
    repositories {
        google()
        mavenCentral()
        gradlePluginPortal()
    }
}

dependencyResolutionManagement {
    repositories {
        google()
        mavenCentral()
    }
}

rootProject.name = "compose-rs-android"
// The library: ComposeRsView and its Kotlin glue (an AAR).
include(":compose-rs")
// The examples (counter, todo, gallery) in a Kotlin Compose app.
include(":demo")
// The benchmark (docs/12): the same app in Kotlin Compose and in compose-rs.
include(":bench-compose")
include(":bench-rs")
