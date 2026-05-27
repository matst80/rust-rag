package com.rustrag.app

import android.app.Application
import androidx.appfunctions.service.AppFunctionConfiguration
import com.rustrag.app.data.AppSearchHelper
import com.rustrag.app.data.RagApiService
import com.rustrag.app.data.TokenManager
import com.rustrag.app.data.GemmaManager

import com.rustrag.app.data.WidgetUpdateWorker

class RagApplication : Application(), AppFunctionConfiguration.Provider {

    lateinit var tokenManager: TokenManager
    lateinit var apiService: RagApiService
    lateinit var appSearchHelper: AppSearchHelper
    lateinit var gemmaManager: GemmaManager

    override fun onCreate() {
        super.onCreate()
        tokenManager = TokenManager(applicationContext)
        apiService = RagApiService(tokenManager)
        appSearchHelper = AppSearchHelper(applicationContext)
        gemmaManager = GemmaManager(applicationContext)

        // Schedule periodic widget updates and trigger a one-time immediate sync on app startup
        WidgetUpdateWorker.schedule(applicationContext)
        WidgetUpdateWorker.runOnce(applicationContext)
    }

    override val appFunctionConfiguration: AppFunctionConfiguration =
        AppFunctionConfiguration.Builder()
            .addEnclosingClassFactory(RagAppFunctions::class.java) {
                RagAppFunctions(apiService, tokenManager, applicationContext)
            }
            .build()
}

