package com.rustrag.app

import android.content.Context
import android.content.Intent
import android.net.Uri
import androidx.core.content.pm.ShortcutInfoCompat
import androidx.core.content.pm.ShortcutManagerCompat
import androidx.core.graphics.drawable.IconCompat

object ShortcutsHelper {
    fun setupShortcuts(context: Context) {
        val newNoteIntent = Intent(context, ShareActivity::class.java).apply {
            action = Intent.ACTION_SEND
            type = "text/plain"
            putExtra(Intent.EXTRA_TEXT, "")
            addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_CLEAR_TOP)
        }

        val searchIntent = Intent(context, MainActivity::class.java).apply {
            action = Intent.ACTION_VIEW
            data = Uri.parse("rustrag://search")
            addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_CLEAR_TOP)
        }

        val newNoteShortcut = ShortcutInfoCompat.Builder(context, "new_note")
            .setShortLabel("New Note")
            .setLongLabel("Create a new note")
            .setIcon(IconCompat.createWithResource(context, R.drawable.ic_brain))
            .setIntent(newNoteIntent)
            .build()

        val searchShortcut = ShortcutInfoCompat.Builder(context, "search_brain")
            .setShortLabel("Search Brain")
            .setLongLabel("Search your second brain")
            .setIcon(IconCompat.createWithResource(context, R.drawable.ic_brain))
            .setIntent(searchIntent)
            .build()

        ShortcutManagerCompat.setDynamicShortcuts(context, listOf(newNoteShortcut, searchShortcut))
    }
}
