package com.rustrag.app

import android.app.Application
import androidx.appfunctions.service.AppFunctionConfiguration
import com.rustrag.app.data.AppSearchHelper
import com.rustrag.app.data.RagApiService
import com.rustrag.app.data.TokenManager
import com.rustrag.app.data.GemmaManager

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
    }

    override val appFunctionConfiguration: AppFunctionConfiguration =
        AppFunctionConfiguration.Builder()
            .addEnclosingClassFactory(RagAppFunctions::class.java) {
                RagAppFunctions(apiService)
            }
            .build()
}

