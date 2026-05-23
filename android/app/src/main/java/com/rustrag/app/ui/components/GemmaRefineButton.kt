package com.rustrag.app.ui.components

import android.net.Uri
import android.widget.Toast
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.AutoAwesome
import androidx.compose.material.icons.filled.Check
import androidx.compose.material.icons.filled.Warning
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.rustrag.app.data.GemmaManager
import com.rustrag.app.data.GemmaStatus
import com.rustrag.app.ui.theme.DarkAccentColor
import kotlinx.coroutines.launch
import kotlinx.coroutines.Job

@Composable
fun GemmaRefineButton(
    content: String,
    gemmaManager: GemmaManager,
    onAccept: (String) -> Unit,
    modifier: Modifier = Modifier
) {
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    
    val status by gemmaManager.status.collectAsState()
    val importProgress by gemmaManager.importProgress.collectAsState()
    val errorMsg by gemmaManager.error.collectAsState()
    
    var showImportDialog by remember { mutableStateOf(false) }
    var showRefineDialog by remember { mutableStateOf(false) }
    var instruction by remember { mutableStateOf("") }
    var refinedText by remember { mutableStateOf<String?>(null) }
    var isGenerating by remember { mutableStateOf(false) }

    var activeTab by remember { mutableStateOf(0) } // 0 = URL, 1 = File
    var modelUrl by remember { mutableStateOf("https://huggingface.co/litert-community/gemma-4-E2B-it-litert-lm/resolve/main/gemma-4-E2B-it.litertlm") }
    var activeJob by remember { mutableStateOf<Job?>(null) }

    val filePickerLauncher = rememberLauncherForActivityResult(
        contract = ActivityResultContracts.GetContent()
    ) { uri: Uri? ->
        if (uri != null) {
            scope.launch {
                val success = gemmaManager.importModel(uri)
                if (success) {
                    Toast.makeText(context, "Gemma model imported successfully!", Toast.LENGTH_SHORT).show()
                    showImportDialog = false
                    showRefineDialog = true
                } else {
                    Toast.makeText(context, "Import failed. Check logs.", Toast.LENGTH_LONG).show()
                }
            }
        }
    }

    Column(modifier = modifier) {
        Button(
            onClick = {
                gemmaManager.checkModelStatus()
                if (status == GemmaStatus.NOT_IMPORTED) {
                    showImportDialog = true
                } else {
                    showRefineDialog = true
                }
            },
            colors = ButtonDefaults.buttonColors(
                containerColor = MaterialTheme.colorScheme.primaryContainer,
                contentColor = MaterialTheme.colorScheme.onPrimaryContainer
            ),
            modifier = Modifier.padding(vertical = 4.dp)
        ) {
            Icon(
                imageVector = Icons.Default.AutoAwesome,
                contentDescription = "Refine with AI",
                modifier = Modifier.size(18.dp)
            )
            Spacer(modifier = Modifier.width(8.dp))
            Text(
                text = "Refine with AI",
                style = MaterialTheme.typography.labelMedium.copy(fontWeight = FontWeight.Bold)
            )
        }

        // 1. Import Model Dialog
        if (showImportDialog) {
            AlertDialog(
                onDismissRequest = { if (status != GemmaStatus.INITIALIZING) showImportDialog = false },
                title = {
                    Text(
                        text = "Local Gemma AI Model",
                        style = MaterialTheme.typography.titleMedium.copy(fontWeight = FontWeight.Bold)
                    )
                },
                text = {
                    Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
                        if (status != GemmaStatus.INITIALIZING) {
                            TabRow(
                                selectedTabIndex = activeTab,
                                containerColor = Color.Transparent,
                                contentColor = DarkAccentColor
                            ) {
                                Tab(
                                    selected = activeTab == 0,
                                    onClick = { activeTab = 0 },
                                    text = { Text("Download URL", color = if (activeTab == 0) DarkAccentColor else Color.Gray) }
                                )
                                Tab(
                                    selected = activeTab == 1,
                                    onClick = { activeTab = 1 },
                                    text = { Text("Select Local File", color = if (activeTab == 1) DarkAccentColor else Color.Gray) }
                                )
                            }
                        }

                        Spacer(modifier = Modifier.height(4.dp))

                        if (status == GemmaStatus.INITIALIZING) {
                            val progressText = if (importProgress >= 0f) {
                                "${(importProgress * 100).toInt()}%"
                            } else {
                                "Downloading..."
                            }
                            Text(
                                text = "Importing model... $progressText",
                                style = MaterialTheme.typography.bodySmall.copy(fontFamily = FontFamily.Monospace)
                            )
                            if (importProgress >= 0f) {
                                LinearProgressIndicator(
                                    progress = { importProgress },
                                    modifier = Modifier.fillMaxWidth(),
                                    color = DarkAccentColor
                                )
                            } else {
                                LinearProgressIndicator(
                                    modifier = Modifier.fillMaxWidth(),
                                    color = DarkAccentColor
                                )
                            }
                        } else {
                            if (activeTab == 0) {
                                Text(
                                    text = "Enter a direct URL to download the Gemma .litertlm model file (typically 1.5 - 3GB size):",
                                    style = MaterialTheme.typography.bodyMedium
                                )
                                ModernTextField(
                                    value = modelUrl,
                                    onValueChange = { modelUrl = it },
                                    label = { Text("Model direct URL") },
                                    placeholder = { Text("https://example.com/gemma.litertlm") },
                                    singleLine = true,
                                    modifier = Modifier.fillMaxWidth()
                                )
                            } else {
                                Text(
                                    text = "Choose a pre-downloaded Gemma .litertlm model file from your device storage.",
                                    style = MaterialTheme.typography.bodyMedium
                                )
                            }
                        }

                        errorMsg?.let { err ->
                            Spacer(modifier = Modifier.height(4.dp))
                            Row(
                                verticalAlignment = Alignment.CenterVertically,
                                horizontalArrangement = Arrangement.spacedBy(8.dp),
                                modifier = Modifier
                                    .fillMaxWidth()
                                    .background(MaterialTheme.colorScheme.errorContainer.copy(alpha = 0.2f), RoundedCornerShape(8.dp))
                                    .padding(8.dp)
                            ) {
                                Icon(Icons.Default.Warning, contentDescription = "Error", tint = MaterialTheme.colorScheme.error)
                                Text(
                                    text = err,
                                    style = MaterialTheme.typography.bodySmall,
                                    color = MaterialTheme.colorScheme.error
                                )
                            }
                        }
                    }
                },
                confirmButton = {
                    if (status == GemmaStatus.INITIALIZING) {
                        Button(
                            onClick = {
                                activeJob?.cancel()
                                activeJob = null
                                gemmaManager.deleteModelFile()
                            },
                            colors = ButtonDefaults.buttonColors(
                                containerColor = MaterialTheme.colorScheme.error,
                                contentColor = MaterialTheme.colorScheme.onError
                            )
                        ) {
                            Text("Cancel Download")
                        }
                    } else {
                        if (activeTab == 0) {
                            Button(
                                onClick = {
                                    if (modelUrl.isNotBlank()) {
                                        activeJob = scope.launch {
                                            val success = gemmaManager.downloadModel(modelUrl.trim())
                                            if (success) {
                                                Toast.makeText(context, "Gemma model downloaded and imported!", Toast.LENGTH_SHORT).show()
                                                showImportDialog = false
                                                showRefineDialog = true
                                            } else {
                                                Toast.makeText(context, "Download failed.", Toast.LENGTH_LONG).show()
                                            }
                                            activeJob = null
                                        }
                                    }
                                },
                                enabled = modelUrl.isNotBlank()
                            ) {
                                Text("Download")
                            }
                        } else {
                            Button(
                                onClick = { filePickerLauncher.launch("*/*") }
                            ) {
                                Text("Select File")
                            }
                        }
                    }
                },
                dismissButton = {
                    if (status != GemmaStatus.INITIALIZING) {
                        TextButton(onClick = { showImportDialog = false }) {
                            Text("Cancel")
                        }
                    }
                }
            )
        }

        // 2. AI Refinement Prompt & Generation Dialog
        if (showRefineDialog) {
            AlertDialog(
                onDismissRequest = { if (!isGenerating) { showRefineDialog = false; refinedText = null } },
                title = {
                    Row(
                        verticalAlignment = Alignment.CenterVertically,
                        horizontalArrangement = Arrangement.spacedBy(8.dp)
                    ) {
                        Icon(
                            imageVector = Icons.Default.AutoAwesome,
                            contentDescription = "Refine",
                            tint = DarkAccentColor
                        )
                        Text(
                            text = "AI Refinement",
                            style = MaterialTheme.typography.titleMedium.copy(
                                fontWeight = FontWeight.Bold,
                                color = MaterialTheme.colorScheme.primary
                            )
                        )
                    }
                },
                text = {
                    Column(
                        verticalArrangement = Arrangement.spacedBy(14.dp),
                        modifier = Modifier.fillMaxWidth()
                    ) {
                        if (refinedText == null) {
                            Text(
                                text = "Enter a refinement instruction for the text below:",
                                style = MaterialTheme.typography.bodyMedium
                            )
                            ModernTextField(
                                value = instruction,
                                onValueChange = { instruction = it },
                                label = { Text("e.g. Summarize as list, Make professional") },
                                singleLine = true,
                                modifier = Modifier.fillMaxWidth()
                            )
                        } else {
                            Text(
                                text = "Refinement Suggestion:",
                                style = MaterialTheme.typography.bodyMedium.copy(fontWeight = FontWeight.Bold)
                            )
                            Box(
                                modifier = Modifier
                                    .fillMaxWidth()
                                    .heightIn(max = 240.dp)
                                    .background(Color(0xFF090A0E), RoundedCornerShape(12.dp))
                                    .border(1.dp, DarkAccentColor.copy(alpha = 0.3f), RoundedCornerShape(12.dp))
                                    .padding(12.dp)
                            ) {
                                val scrollState = rememberScrollState()
                                Column(
                                    modifier = Modifier
                                        .fillMaxWidth()
                                        .verticalScroll(scrollState)
                                ) {
                                    Text(
                                        text = refinedText ?: "",
                                        style = MaterialTheme.typography.bodyMedium.copy(
                                            fontFamily = FontFamily.Monospace,
                                            lineHeight = 20.sp,
                                            color = Color.White
                                        )
                                    )
                                    if (isGenerating) {
                                        CircularProgressIndicator(
                                            modifier = Modifier
                                                .size(16.dp)
                                                .padding(top = 4.dp),
                                            color = DarkAccentColor,
                                            strokeWidth = 2.dp
                                        )
                                    }
                                }
                            }
                        }
                    }
                },
                confirmButton = {
                    Row(
                        horizontalArrangement = Arrangement.spacedBy(8.dp)
                    ) {
                        if (refinedText == null) {
                            Button(
                                onClick = {
                                    if (instruction.isNotBlank()) {
                                        isGenerating = true
                                        refinedText = ""
                                        scope.launch {
                                            val prompt = """You are a helpful writing assistant. 
Refine the following text based on this instruction: "$instruction"
Keep the original markdown formatting where appropriate.
Output ONLY the refined text. No explanations.

ORIGINAL TEXT:
$content

REFINED TEXT:"""
                                            gemmaManager.generateStreaming(prompt) { partialText, done ->
                                                scope.launch {
                                                    refinedText = partialText
                                                    if (done) {
                                                        isGenerating = false
                                                    }
                                                }
                                            }
                                        }
                                    }
                                },
                                enabled = instruction.isNotBlank() && !isGenerating
                            ) {
                                Text("Refine")
                            }
                        } else if (!isGenerating) {
                            Button(
                                onClick = {
                                    refinedText?.let { onAccept(it) }
                                    showRefineDialog = false
                                    refinedText = null
                                    instruction = ""
                                }
                            ) {
                                Icon(Icons.Default.Check, contentDescription = "Accept", modifier = Modifier.size(16.dp))
                                Spacer(modifier = Modifier.width(4.dp))
                                Text("Accept")
                            }
                        }
                    }
                },
                dismissButton = {
                    if (refinedText != null && !isGenerating) {
                        TextButton(
                            onClick = {
                                refinedText = null
                            }
                        ) {
                            Text("Retry")
                        }
                    } else if (!isGenerating) {
                        TextButton(
                            onClick = {
                                showRefineDialog = false
                                refinedText = null
                            }
                        ) {
                            Text("Cancel")
                        }
                    }
                }
            )
        }
    }
}
