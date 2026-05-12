"use client"

import { useState, useEffect } from "react"
import { Tabs, TabsList, TabsTrigger, TabsContent } from "@/components/ui/tabs"
import { Editor } from "@monaco-editor/react"
import Form from "@rjsf/core"
import validator from "@rjsf/validator-ajv8"
import { RJSFSchema } from "@rjsf/utils"
import { Code2, FormInput, AlertCircle } from "lucide-react"
import { AiExtractButton } from "../ai/ai-extract-button"

interface StructuredDataEditorProps {
  schema: any
  value: any
  onValidityChange?: (isValid: boolean) => void
  typeName: string
  content?: string
}

export function StructuredDataEditor({
  schema,
  value,
  onChange,
  onValidityChange,
  typeName,
  content,
}: StructuredDataEditorProps) {
  const [jsonText, setJsonText] = useState(JSON.stringify(value || {}, null, 2))
  const [error, setError] = useState<string | null>(null)
  const [validationErrors, setValidationErrors] = useState<string[] | null>(null)
  const [activeTab, setActiveTab] = useState("form")

  // Sync internal text state when external value changes
  useEffect(() => {
    try {
      const currentText = JSON.stringify(value || {}, null, 2)
      if (JSON.stringify(JSON.parse(jsonText)) !== JSON.stringify(value)) {
        setJsonText(currentText)
      }
    } catch {
      // If current text is invalid JSON, don't overwrite it while user is typing
    }
  }, [value])

  const handleCodeChange = (text: string | undefined) => {
    const newText = text || "{}"
    setJsonText(newText)
    try {
      const parsed = JSON.parse(newText)
      setError(null)

      // Also validate against schema using the same validator as the form
      const result = validator.validateFormData(parsed, schema as RJSFSchema)
      const isValid = result.errors.length === 0
      setValidationErrors(isValid ? null : result.errors.map(e => (e as any).stack || e.message))

      onChange(parsed)
      onValidityChange?.(isValid)
    } catch (e) {
      const msg = (e as Error).message
      setError(msg)
      setValidationErrors(null)
      onValidityChange?.(false)
    }
  }

  const handleFormChange = (data: any, isValid?: boolean, errors: any[] = []) => {
    setError(null)

    let finalValid = isValid
    let finalErrors = errors

    // If no validation state provided (e.g. from AI extraction), run validation now
    if (finalValid === undefined) {
      const result = validator.validateFormData(data, schema as RJSFSchema)
      finalValid = result.errors.length === 0
      finalErrors = result.errors
    }

    setValidationErrors(finalErrors.length > 0 ? finalErrors.map(e => (e as any).stack || e.message) : null)
    onChange(data)
    onValidityChange?.(finalValid ?? false)
    setJsonText(JSON.stringify(data, null, 2))
  }

  const handleExtractError = (raw: string) => {
    setJsonText(raw)
    setError("AI produced invalid JSON. You can try to fix it manually in the code editor below.")
    setValidationErrors(null)
    setActiveTab("code")
  }

  return (
    <div className="flex flex-col gap-3">
      <Tabs value={activeTab} onValueChange={setActiveTab} className="w-full">
        <div className="flex items-center justify-between">
          <TabsList className="grid w-[200px] grid-cols-2">
            <TabsTrigger value="form" className="flex items-center gap-2">
              <FormInput className="size-3.5" />
              Form
            </TabsTrigger>
            <TabsTrigger value="code" className="flex items-center gap-2">
              <Code2 className="size-3.5" />
              Code
            </TabsTrigger>
          </TabsList>

          <div className="flex items-center gap-3">
            {content && (
              <AiExtractButton
                content={content}
                schema={schema}
                onExtract={(data) => {
                  handleFormChange(data)
                  setActiveTab("form")
                }}
                onExtractError={handleExtractError}
              />
            )}
          </div>
        </div>

        {error && (
          <div className="mt-3 p-3 bg-red-500/10 border border-red-500/20 rounded-md flex items-start gap-3 animate-in slide-in-from-top-1 duration-200 shadow-sm">
            <AlertCircle className="size-4 text-red-500 mt-0.5 shrink-0" />
            <div className="space-y-1">
              <div className="text-[10px] font-black text-red-500 uppercase tracking-[2px] font-mono leading-none">Syntax Error</div>
              <div className="text-[11px] font-mono text-red-400/90 leading-tight">{error}</div>
            </div>
          </div>
        )}

        {validationErrors && validationErrors.length > 0 && (
          <div className="mt-3 p-3 bg-amber-500/10 border border-amber-500/20 rounded-md flex items-start gap-3 animate-in slide-in-from-top-1 duration-200 shadow-sm">
            <AlertCircle className="size-4 text-amber-500 mt-0.5 shrink-0" />
            <div className="space-y-1">
              <div className="text-[10px] font-black text-amber-500 uppercase tracking-[2px] font-mono leading-none">Validation Error</div>
              <ul className="list-none p-0 m-0">
                {validationErrors.map((err, idx) => (
                  <li key={idx} className="text-[11px] font-mono text-amber-400/90 leading-tight flex gap-1.5">
                    <span className="opacity-50">•</span>
                    {err}
                  </li>
                ))}
              </ul>
            </div>
          </div>
        )}

        <TabsContent value="form" className="mt-3 min-h-[300px]">
          <div className="rjsf-wrapper">
            <Form
              schema={schema as RJSFSchema}
              validator={validator}
              formData={value || {}}
              onChange={(e) => handleFormChange(e.formData, e.errors.length === 0, e.errors)}
              children={<></>} // Hide default submit button
              className="space-y-4"
            />
          </div>
        </TabsContent>

        <TabsContent value="code" className="mt-3 border rounded-md overflow-hidden bg-card">
          <Editor
            height="400px"
            defaultLanguage="json"
            theme="vs-dark"
            value={jsonText}
            onChange={handleCodeChange}
            options={{
              minimap: { enabled: false },
              fontSize: 12,
              scrollBeyondLastLine: false,
              automaticLayout: true,
              formatOnPaste: true,
              formatOnType: true,
            }}
            onMount={(editor, monaco) => {
              // Configure Monaco to use the JSON schema for validation
              monaco.languages.json.jsonDefaults.setDiagnosticsOptions({
                validate: true,
                schemas: [
                  {
                    uri: "http://rust-rag/schema.json",
                    fileMatch: ["*"],
                    schema: schema,
                  },
                ],
              })
            }}
          />
        </TabsContent>
      </Tabs>

      <style jsx global>{`
        .rjsf-wrapper .form-group { margin-bottom: 1.25rem; }
        .rjsf-wrapper label { 
          display: block; 
          font-family: var(--font-mono), monospace;
          font-size: 0.75rem; 
          font-weight: 700; 
          text-transform: uppercase;
          letter-spacing: 0.1em;
          margin-bottom: 0.5rem; 
          color: var(--muted-foreground); 
        }
        .rjsf-wrapper input, .rjsf-wrapper select, .rjsf-wrapper textarea {
          width: 100%;
          border-radius: 0.375rem;
          border: 1px solid var(--border);
          background-color: var(--background);
          padding: 0.5rem 0.75rem;
          font-size: 0.875rem;
          color: var(--foreground);
        }
        .rjsf-wrapper input:focus { outline: none; ring: 2px; ring-color: var(--primary); border-color: var(--primary); }
        .rjsf-wrapper .help-block { font-size: 0.75rem; color: var(--muted-foreground); margin-top: 0.25rem; }
        
        /* Array & Object Styling */
        .rjsf-wrapper fieldset { border: 1px solid var(--border); padding: 1rem; border-radius: 0.5rem; margin-bottom: 1rem; background: var(--muted)/20; }
        .rjsf-wrapper fieldset legend { 
          font-family: var(--font-mono), monospace;
          font-size: 0.75rem; 
          font-weight: 700; 
          text-transform: uppercase; 
          letter-spacing: 0.1em; 
          padding: 0 0.5rem; 
          color: var(--muted-foreground); 
        }
        
        /* Buttons (Add, Remove, Move) */
        .rjsf-wrapper .btn-add::after { content: "+ ADD ITEM"; font-size: 10px; }
        .rjsf-wrapper .btn-add { 
          background-color: var(--primary); color: var(--primary-foreground); 
          padding: 4px 12px; border-radius: 2px; font-weight: 900;
          letter-spacing: 1px; margin-top: 8px; cursor: pointer; border: none;
          text-transform: uppercase;
        }
        
        .rjsf-wrapper .btn-danger::after { content: "✕"; font-size: 14px; }
        .rjsf-wrapper .btn-danger { 
          background-color: #ef4444; color: white; 
          width: 24px; height: 24px; border-radius: 2px; cursor: pointer; border: none;
          display: flex; align-items: center; justify-content: center;
          font-weight: bold;
          font-size: 0;
        }
        
        .rjsf-wrapper .array-item { 
          display: flex; gap: 12px; align-items: flex-end; margin-bottom: 12px; 
          border-bottom: 1px solid var(--border); padding-bottom: 12px; 
        }
        .rjsf-wrapper .array-item > div:first-child { flex: 1; }
        .rjsf-wrapper .array-item-toolbox { display: flex; gap: 4px; padding-bottom: 6px; }
        .rjsf-wrapper .btn-group { display: flex; gap: 2px; padding-inline: 5px; }
        .rjsf-wrapper .btn-group button { 
          background: var(--muted); 
          color: var(--foreground); 
          border: none; 
          width: 24px; 
          height: 24px; 
          font-size: 0; 
          cursor: pointer; 
          border-radius: 2px;
          display: flex; 
          align-items: center; 
          justify-content: center;
          font-weight: bold;
        }
        .rjsf-wrapper .btn-group button i { display: none; }
        .rjsf-wrapper .rjsf-array-item-move-up::after { content: "↑"; font-size: 12px; }
        .rjsf-wrapper .rjsf-array-item-move-down::after { content: "↓"; font-size: 12px; }
        .rjsf-wrapper .btn-group button:hover { background: var(--primary); color: var(--primary-foreground); }
        .rjsf-wrapper .btn-group button:disabled { opacity: 0.3; cursor: not-allowed; }
        
        .rjsf-wrapper .error-detail { color: var(--destructive); font-size: 0.75rem; list-style: none; padding: 0; margin-top: 0.25rem; }
      `}</style>
    </div>
  )
}
