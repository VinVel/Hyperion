import org.jetbrains.compose.desktop.application.dsl.TargetFormat
import org.jetbrains.kotlin.gradle.dsl.JvmTarget

plugins {
    alias(libs.plugins.kotlinMultiplatform)
    alias(libs.plugins.androidMultiplatformLibrary)
    alias(libs.plugins.composeMultiplatform)
    alias(libs.plugins.composeCompiler)
    alias(libs.plugins.composeHotReload)
}

kotlin {
    androidLibrary {
        namespace = "net.velcore.hyperion.composeapplibrary"
        compileSdk = libs.versions.android.compileSdk.get().toInt()

        compilerOptions {
            jvmTarget.set(JvmTarget.JVM_11)
        }

        androidResources {
            enable = true
        }
    }

//    listOf(
//        iosArm64(),
//        iosSimulatorArm64()
//    ).forEach { iosTarget ->
//        iosTarget.binaries.framework {
//            baseName = "ComposeApp"
//            isStatic = true
//        }
//    }

    jvm()

    js {
        browser()
        binaries.executable()
    }

    /*@OptIn(ExperimentalWasmDsl::class)
    *wasmJs {
    *    browser()
    *    binaries.executable()
    *}
    */
    sourceSets {
        androidMain.dependencies {
            implementation(libs.compose.uiToolingPreview)
            implementation(libs.androidx.activity.compose)
            implementation(libs.ktor.client.okhttp)
            implementation(libs.trixnity.client.repository.room)

            // Media
            implementation(libs.trixnity.client.media.okio)

            // Crypto Driver (Vodozemac)
            implementation(libs.trixnity.client.cryptodriver.vodozemac)

        }
        commonMain.dependencies {
            implementation(libs.compose.runtime)
            implementation(libs.compose.foundation)
            implementation(libs.compose.material3)
            implementation(libs.compose.ui)
            implementation(libs.compose.components.resources)
            implementation(libs.compose.uiToolingPreview)
            implementation(libs.androidx.lifecycle.viewmodelCompose)
            implementation(libs.androidx.lifecycle.runtimeCompose)
            implementation(libs.ktor.client.core)
            implementation(libs.composables.icons.lucide.cmp)
            implementation(libs.composeunstyled)
            implementation(project.dependencies.platform(libs.trixnity.bom))

            // Core Client
            implementation(libs.trixnity.client)
            implementation(libs.trixnity.core)

            // Crypto API (plattformunabhängig)
            implementation(libs.trixnity.crypto)
            implementation(libs.trixnity.crypto.core)
            implementation(libs.trixnity.crypto.driver)

            // Client-Server API (für Login etc.)
            implementation(libs.trixnity.clientserverapi.client)
            implementation(libs.trixnity.clientserverapi.model)


        }
        commonTest.dependencies {
            implementation(libs.kotlin.test)
        }
        jvmMain.dependencies {
            implementation(compose.desktop.currentOs)
            implementation(libs.kotlinx.coroutinesSwing)
            implementation(libs.ktor.client.okhttp)
            implementation(libs.trixnity.client.repository.room)

            // Media
            implementation(libs.trixnity.client.media.okio)

            // Crypto Driver (Vodozemac)
            implementation(libs.trixnity.client.cryptodriver.vodozemac)
        }

        jsMain.dependencies {
            implementation(libs.ktor.client.js)
            implementation(libs.trixnity.client)
            implementation(libs.trixnity.core)
            implementation(libs.trixnity.crypto)
            implementation(libs.trixnity.client.repository.indexeddb)
            implementation(libs.trixnity.client.media.indexeddb)
            implementation(libs.trixnity.client.cryptodriver.vodozemac)
        }
    }
}

dependencies {
    androidRuntimeClasspath(libs.compose.uiTooling)
}

compose.desktop {
    application {
        mainClass = "net.velcore.hyperion.MainKt"

        nativeDistributions {
            targetFormats(TargetFormat.Dmg, TargetFormat.Msi, TargetFormat.Deb)
            packageName = "net.velcore.hyperion"
            packageVersion = "1.0.0"
        }
    }
}

dependencyLocking {
    lockAllConfigurations()
}

