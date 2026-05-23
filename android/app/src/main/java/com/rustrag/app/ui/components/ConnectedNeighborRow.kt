package com.rustrag.app.ui.components

import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.*
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.rustrag.app.data.EntryNeighbor
import com.rustrag.app.data.StoreAnalysisVerdict
import java.util.*

@Composable
fun ConnectedNeighborRow(
    neighbor: EntryNeighbor,
    verdict: StoreAnalysisVerdict?,
    onClick: () -> Unit,
    modifier: Modifier = Modifier
) {
    val isAgrees = verdict?.relation?.lowercase() == "agrees" || verdict?.relation?.lowercase() == "agree"
    val isUnrelated = verdict?.relation?.lowercase() == "unrelated"

    val agreesColor = Color(0xFF00C853) // MD3 clean green
    val stateColor = when {
        isAgrees -> agreesColor
        else -> MaterialTheme.colorScheme.onSurfaceVariant
    }

    Column(
        modifier = modifier
            .fillMaxWidth()
            .clickable(onClick = onClick)
            .padding(vertical = 12.dp)
    ) {
        // Title & Relationship Row
        Row(
            modifier = Modifier.fillMaxWidth(),
            verticalAlignment = Alignment.CenterVertically
        ) {
            Text(
                text = neighbor.title ?: neighbor.id.take(8),
                style = MaterialTheme.typography.titleSmall.copy(
                    fontWeight = FontWeight.Bold
                ),
                color = MaterialTheme.colorScheme.onSurface,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
                modifier = Modifier.weight(1f, fill = false)
            )

            neighbor.relationship?.let { rel ->
                Spacer(modifier = Modifier.width(6.dp))
                Text(
                    text = "(${rel.take(15)})",
                    style = MaterialTheme.typography.labelSmall.copy(
                        fontFamily = FontFamily.Monospace,
                        color = MaterialTheme.colorScheme.onSurfaceVariant.copy(alpha = 0.6f)
                    ),
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis
                )
            }
        }

        Spacer(modifier = Modifier.height(6.dp))

        // Badge Row: Connection Type + Verdict
        Row(
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(8.dp)
        ) {
            val relType = neighbor.sourceType ?: "similarity"
            Surface(
                shape = CircleShape,
                color = stateColor.copy(alpha = 0.1f),
                border = BorderStroke(
                    0.5.dp,
                    stateColor.copy(alpha = 0.3f)
                )
            ) {
                Text(
                    text = relType.uppercase(),
                    modifier = Modifier.padding(horizontal = 8.dp, vertical = 2.dp),
                    style = MaterialTheme.typography.labelSmall.copy(
                        fontWeight = FontWeight.Bold,
                        fontFamily = FontFamily.Monospace,
                        color = stateColor,
                        fontSize = 9.sp
                    ),
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis
                )
            }

            if (isAgrees) {
                Text(
                    text = "🟢 AGREES",
                    style = MaterialTheme.typography.labelSmall.copy(
                        fontWeight = FontWeight.Bold,
                        color = agreesColor,
                        fontFamily = FontFamily.Monospace,
                        fontSize = 9.sp
                    )
                )

                verdict?.confidence?.let { conf ->
                    val pct = (conf * 100).coerceIn(0f, 100f)
                    Text(
                        text = String.format(Locale.US, "%.0f%% conf", pct),
                        style = MaterialTheme.typography.labelSmall.copy(
                            fontFamily = FontFamily.Monospace,
                            color = agreesColor.copy(alpha = 0.8f),
                            fontSize = 9.sp
                        )
                    )
                }
            } else if (isUnrelated) {
                Text(
                    text = "👻 UNRELATED",
                    style = MaterialTheme.typography.labelSmall.copy(
                        fontWeight = FontWeight.Bold,
                        color = MaterialTheme.colorScheme.onSurfaceVariant.copy(alpha = 0.6f),
                        fontFamily = FontFamily.Monospace,
                        fontSize = 9.sp
                    )
                )
            } else {
                verdict?.relation?.let { rel ->
                    Text(
                        text = "❓ ${rel.uppercase()}",
                        style = MaterialTheme.typography.labelSmall.copy(
                            fontWeight = FontWeight.Bold,
                            color = MaterialTheme.colorScheme.onSurfaceVariant.copy(alpha = 0.6f),
                            fontFamily = FontFamily.Monospace,
                            fontSize = 9.sp
                        )
                    )
                }
            }
        }

        // Reason description if present
        verdict?.reason?.let { reason ->
            Spacer(modifier = Modifier.height(8.dp))
            Text(
                text = reason,
                style = MaterialTheme.typography.bodyMedium.copy(
                    lineHeight = 18.sp
                ),
                color = MaterialTheme.colorScheme.onSurfaceVariant
            )
        }
    }
}
