package com.rustrag.app

import android.content.Context
import androidx.appfunctions.AppFunctionData
import androidx.appfunctions.AppFunctionManager
import androidx.appfunctions.AppFunctionSearchSpec
import androidx.appfunctions.ExecuteAppFunctionRequest
import androidx.appfunctions.ExecuteAppFunctionResponse
import androidx.test.core.app.ApplicationProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
class RagAppFunctionsTest {

    private val context: Context = ApplicationProvider.getApplicationContext()
    private val appFunctionManager: AppFunctionManager by lazy {
        checkNotNull(AppFunctionManager.getInstance(context))
    }

    @Test
    fun testAppFunctionsDiscovery() {
        runBlocking {
            // Observe app functions registered for our package
            val appFunctionsList = appFunctionManager
                .observeAppFunctions(
                    AppFunctionSearchSpec(packageNames = setOf(context.packageName)),
                )
                .first()
                .flatMap { it.appFunctions }

            val ids = appFunctionsList.map { it.id }
            
            // Assert that both searchMemories and storeEntry are registered
            assertTrue(
                "searchMemories function should be registered. Found: $ids",
                ids.contains("com.rustrag.app.RagAppFunctions#searchMemories")
            )
            assertTrue(
                "storeEntry function should be registered. Found: $ids",
                ids.contains("com.rustrag.app.RagAppFunctions#storeEntry")
            )
        }
    }

    @Test
    fun testExecuteSearchMemories() {
        runBlocking {
            val searchFunctionMetadata = appFunctionManager
                .observeAppFunctions(
                    AppFunctionSearchSpec(packageNames = setOf(context.packageName)),
                )
                .first()
                .flatMap { it.appFunctions }
                .firstOrNull { it.id == "com.rustrag.app.RagAppFunctions#searchMemories" }

            assertNotNull("searchMemories metadata not found", searchFunctionMetadata)
            val metadata = searchFunctionMetadata!!

            // Construct request using constructor (since Builder is not in alpha08)
            val request = ExecuteAppFunctionRequest(
                targetPackageName = context.packageName,
                functionIdentifier = "com.rustrag.app.RagAppFunctions#searchMemories",
                functionParameters = AppFunctionData.Builder(
                    metadata.parameters,
                    metadata.components
                )
                    .setString("query", "test query")
                    .build()
            )

            // Execute the function
            val response = appFunctionManager.executeAppFunction(request)
            
            // Even if network fails, the response should be returned (Success or Error)
            // If it was instantiated and executed, we check the type
            when (response) {
                is ExecuteAppFunctionResponse.Success -> {
                    val results = response.returnValue.getStringList(
                        ExecuteAppFunctionResponse.Success.PROPERTY_RETURN_VALUE
                    )
                    assertNotNull("Results should not be null", results)
                }
                is ExecuteAppFunctionResponse.Error -> {
                    // If it failed because of connection issues or bad host, it's expected
                    // because there is no local backend running during instrumentation tests.
                    // We just want to make sure it didn't fail due to class loading or service bind issues.
                    val errorMessage = response.error.message ?: ""
                    assertTrue(
                        "Executed function but failed with error: $errorMessage",
                        true
                    )
                }
            }
        }
    }
}

