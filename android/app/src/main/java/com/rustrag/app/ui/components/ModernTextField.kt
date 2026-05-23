package com.rustrag.app.ui.components

import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.*
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.unit.dp
import com.rustrag.app.ui.theme.DarkAccentColor

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun ModernTextField(
    value: String,
    onValueChange: (String) -> Unit,
    label: @Composable (() -> Unit)? = null,
    placeholder: @Composable (() -> Unit)? = null,
    leadingIcon: @Composable (() -> Unit)? = null,
    trailingIcon: @Composable (() -> Unit)? = null,
    singleLine: Boolean = true,
    maxLines: Int = if (singleLine) 1 else Int.MAX_VALUE,
    keyboardOptions: KeyboardOptions = KeyboardOptions.Default,
    keyboardActions: KeyboardActions = KeyboardActions.Default,
    modifier: Modifier = Modifier
) {
  //  val shape = RoundedCornerShape(16.dp)

    TextField(
        value = value,
        onValueChange = onValueChange,
        label = label,
        placeholder = placeholder,
        leadingIcon = leadingIcon,
        trailingIcon = trailingIcon,
        singleLine = singleLine,
        maxLines = maxLines,
        keyboardOptions = keyboardOptions,
        keyboardActions = keyboardActions,
       // shape = shape,
        colors = OutlinedTextFieldDefaults.colors(
            focusedContainerColor = Color(0xFF121212),
            unfocusedContainerColor = Color(0xFF090A0E),
            focusedBorderColor = DarkAccentColor,
            unfocusedBorderColor = Color(0xFF2A2B36),
            focusedLabelColor = DarkAccentColor,
            unfocusedLabelColor = Color(0xFF90A4AE),
            cursorColor = DarkAccentColor,
            focusedTextColor = Color.White,
            unfocusedTextColor = Color.White
        ),
        modifier = modifier
    )
}
