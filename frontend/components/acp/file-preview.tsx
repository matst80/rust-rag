"use client";

import React, { useMemo } from "react";
import CodeMirror from "@uiw/react-codemirror";
import { javascript } from "@codemirror/lang-javascript";
import { rust } from "@codemirror/lang-rust";
import { python } from "@codemirror/lang-python";
import { json } from "@codemirror/lang-json";
import { markdown } from "@codemirror/lang-markdown";
import { cpp } from "@codemirror/lang-cpp";
import { html } from "@codemirror/lang-html";
import { css } from "@codemirror/lang-css";
import { sql } from "@codemirror/lang-sql";
import { yaml } from "@codemirror/lang-yaml";
import { go } from "@codemirror/lang-go";
import { java } from "@codemirror/lang-java";
import { php } from "@codemirror/lang-php";
import { X, ExternalLink } from "lucide-react";
import { Button } from "@/components/ui/button";

interface FilePreviewProps {
  path: string;
  content: string;
  startLine: number;
  totalLines: number;
  onClose: () => void;
}

export function FilePreview({
  path,
  content,
  startLine,
  totalLines,
  onClose,
}: FilePreviewProps) {
  const extension = path.split(".").pop()?.toLowerCase();

  const language = useMemo(() => {
    switch (extension) {
      case "js":
      case "jsx":
      case "ts":
      case "tsx":
        return javascript({ jsx: true, typescript: true });
      case "rs":
        return rust();
      case "py":
        return python();
      case "json":
        return json();
      case "md":
        return markdown();
      case "cpp":
      case "h":
      case "hpp":
      case "c":
        return cpp();
      case "html":
        return html();
      case "css":
        return css();
      case "sql":
        return sql();
      case "yaml":
      case "yml":
        return yaml();
      case "go":
        return go();
      case "java":
        return java();
      case "php":
        return php();
      default:
        return undefined;
    }
  }, [extension]);

  return (
    <div className="flex flex-col h-full bg-background border-l border-border shadow-2xl overflow-hidden">
      <div className="flex items-center justify-between px-4 py-2 border-b border-border bg-muted/50">
        <div className="flex flex-col min-w-0">
          <div className="flex items-center gap-2">
            <span className="text-sm font-semibold truncate">{path.split("/").pop()}</span>
            <span className="text-[10px] text-muted-foreground font-mono truncate">{path}</span>
          </div>
          <div className="text-[10px] text-muted-foreground">
            Lines {startLine} - {startLine + content.split("\n").length - 1} of {totalLines}
          </div>
        </div>
        <div className="flex items-center gap-1">
          <Button variant="ghost" size="icon" onClick={onClose} className="size-8">
            <X className="size-4" />
          </Button>
        </div>
      </div>
      <div className="flex-1 overflow-hidden relative">
        <CodeMirror
          value={content}
          height="100%"
          theme="dark"
          extensions={language ? [language] : []}
          basicSetup={{
            lineNumbers: true,
            foldGutter: true,
            highlightActiveLine: true,
            dropCursor: true,
            allowMultipleSelections: true,
            indentOnInput: true,
            syntaxHighlighting: true,
            bracketMatching: true,
            autocompletion: true,
            rectangularSelection: true,
            crosshairCursor: true,
            highlightSelectionMatches: true,
            closeBrackets: true,
          }}
          editable={false}
          className="h-full text-sm"
        />
      </div>
    </div>
  );
}
