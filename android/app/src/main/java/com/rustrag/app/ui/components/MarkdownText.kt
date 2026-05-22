package com.rustrag.app.ui.components

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextDecoration
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import org.commonmark.node.*
import org.commonmark.parser.Parser

@Composable
fun MarkdownText(
    markdown: String,
    modifier: Modifier = Modifier
) {
    val parser = Parser.builder().build()
    val document = parser.parse(markdown)

    Column(modifier = modifier) {
        RenderNodeChildren(document)
    }
}

@Composable
private fun RenderNodeChildren(parentNode: Node) {
    var child = parentNode.firstChild
    while (child != null) {
        when (child) {
            is Heading -> {
                HeadingBlock(child)
            }
            is Paragraph -> {
                ParagraphBlock(child)
            }
            is BlockQuote -> {
                BlockQuoteBlock(child)
            }
            is FencedCodeBlock -> {
                FencedCodeBlockBlock(child)
            }
            is IndentedCodeBlock -> {
                IndentedCodeBlockBlock(child)
            }
            is BulletList -> {
                BulletListBlock(child)
            }
            is OrderedList -> {
                OrderedListBlock(child)
            }
            is ThematicBreak -> {
                HorizontalDivider(
                    modifier = Modifier.padding(vertical = 12.dp),
                    color = MaterialTheme.colorScheme.surfaceVariant
                )
            }
            else -> {
                // Fallback for container nodes
                RenderNodeChildren(child)
            }
        }
        child = child.next
    }
}

@Composable
private fun HeadingBlock(heading: Heading) {
    val fontSize = when (heading.level) {
        1 -> 24.sp
        2 -> 20.sp
        3 -> 18.sp
        else -> 16.sp
    }
    val fontWeight = FontWeight.Bold
    val text = buildAnnotatedText(heading)

    Text(
        text = text,
        fontSize = fontSize,
        fontWeight = fontWeight,
        color = MaterialTheme.colorScheme.primary,
        modifier = Modifier.padding(top = 16.dp, bottom = 8.dp)
    )
}

@Composable
private fun ParagraphBlock(paragraph: Paragraph) {
    val text = buildAnnotatedText(paragraph)
    Text(
        text = text,
        fontSize = 15.sp,
        lineHeight = 22.sp,
        color = MaterialTheme.colorScheme.onBackground,
        modifier = Modifier.padding(vertical = 4.dp)
    )
}

@Composable
private fun BlockQuoteBlock(blockQuote: BlockQuote) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .padding(vertical = 6.dp)
            .clip(RoundedCornerShape(4.dp))
            .background(MaterialTheme.colorScheme.surfaceVariant.copy(alpha = 0.5f))
            .padding(vertical = 8.dp, horizontal = 12.dp)
    ) {
        // Vertical indicator bar
        Box(
            modifier = Modifier
                .width(4.dp)
                .fillMaxHeight()
                .background(MaterialTheme.colorScheme.primary)
                .align(Alignment.CenterVertically)
        )
        Spacer(modifier = Modifier.width(12.dp))
        Column {
            RenderNodeChildren(blockQuote)
        }
    }
}

@Composable
private fun FencedCodeBlockBlock(codeBlock: FencedCodeBlock) {
    Box(
        modifier = Modifier
            .fillMaxWidth()
            .padding(vertical = 6.dp)
            .clip(RoundedCornerShape(6.dp))
            .background(MaterialTheme.colorScheme.surfaceVariant)
            .padding(10.dp)
    ) {
        Text(
            text = codeBlock.literal.trim(),
            fontFamily = FontFamily.Monospace,
            fontSize = 13.sp,
            lineHeight = 18.sp,
            color = MaterialTheme.colorScheme.onSurfaceVariant
        )
    }
}

@Composable
private fun IndentedCodeBlockBlock(codeBlock: IndentedCodeBlock) {
    Box(
        modifier = Modifier
            .fillMaxWidth()
            .padding(vertical = 6.dp)
            .clip(RoundedCornerShape(6.dp))
            .background(MaterialTheme.colorScheme.surfaceVariant)
            .padding(10.dp)
    ) {
        Text(
            text = codeBlock.literal.trim(),
            fontFamily = FontFamily.Monospace,
            fontSize = 13.sp,
            lineHeight = 18.sp,
            color = MaterialTheme.colorScheme.onSurfaceVariant
        )
    }
}

@Composable
private fun BulletListBlock(bulletList: BulletList) {
    Column(modifier = Modifier.padding(start = 12.dp, top = 4.dp, bottom = 4.dp)) {
        var child = bulletList.firstChild
        while (child != null) {
            if (child is ListItem) {
                Row(modifier = Modifier.padding(vertical = 2.dp)) {
                    Text(
                        text = "•",
                        fontSize = 15.sp,
                        color = MaterialTheme.colorScheme.primary,
                        modifier = Modifier.padding(end = 8.dp)
                    )
                    Column {
                        RenderNodeChildren(child)
                    }
                }
            }
            child = child.next
        }
    }
}

@Composable
private fun OrderedListBlock(orderedList: OrderedList) {
    Column(modifier = Modifier.padding(start = 12.dp, top = 4.dp, bottom = 4.dp)) {
        var child = orderedList.firstChild
        var index = orderedList.startNumber
        while (child != null) {
            if (child is ListItem) {
                Row(modifier = Modifier.padding(vertical = 2.dp)) {
                    Text(
                        text = "$index.",
                        fontSize = 15.sp,
                        color = MaterialTheme.colorScheme.primary,
                        modifier = Modifier.padding(end = 8.dp)
                    )
                    Column {
                        RenderNodeChildren(child)
                    }
                }
                index++
            }
            child = child.next
        }
    }
}

private fun buildAnnotatedText(node: Node): AnnotatedString {
    return buildAnnotatedString {
        var child = node.firstChild
        while (child != null) {
            when (child) {
                is Text -> {
                    append(child.literal)
                }
                is StrongEmphasis -> {
                    pushStyle(SpanStyle(fontWeight = FontWeight.Bold))
                    append(buildAnnotatedText(child))
                    pop()
                }
                is Emphasis -> {
                    pushStyle(SpanStyle(fontStyle = FontStyle.Italic))
                    append(buildAnnotatedText(child))
                    pop()
                }
                is Code -> {
                    pushStyle(
                        SpanStyle(
                            fontFamily = FontFamily.Monospace,
                            fontSize = 13.5.sp,
                            background = Color(0x2F808080),
                            color = Color(0xFFEF5350)
                        )
                    )
                    append(child.literal)
                    pop()
                }
                is Link -> {
                    pushStyle(
                        SpanStyle(
                            color = Color(0xFF26C6DA),
                            textDecoration = TextDecoration.Underline,
                            fontWeight = FontWeight.Medium
                        )
                    )
                    append(buildAnnotatedText(child))
                    pop()
                }
                is SoftLineBreak -> {
                    append(" ")
                }
                is HardLineBreak -> {
                    append("\n")
                }
                else -> {
                    append(buildAnnotatedText(child))
                }
            }
            child = child.next
        }
    }
}
