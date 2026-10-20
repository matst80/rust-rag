"use client"

import { use } from "react"
import Link from "next/link"
import { useSchema } from "@/lib/api/hooks"
import { DatabaseView } from "@/components/database/database-view"

interface DatabaseDetailPageProps {
  params: Promise<{ type: string }>
}

export default function DatabaseDetailPage({ params }: DatabaseDetailPageProps) {
  const { type } = use(params)
  const typeName = decodeURIComponent(type)
  const { data: schema } = useSchema(typeName)

  return (
    <div className="mx-auto flex min-h-[calc(100vh-3rem)] w-full max-w-6xl flex-col gap-4 p-4 md:p-8">
      <header className="flex flex-wrap items-baseline justify-between gap-2">
        <div>
          <Link href="/databases" className="font-mono text-[11px] uppercase tracking-widest text-muted-foreground hover:text-primary">
            ← All databases
          </Link>
          <h1 className="mt-1 font-mono text-lg font-black tracking-tight">
            {schema?.title ?? typeName}
            <span className="ml-2 align-middle font-mono text-[11px] font-bold text-muted-foreground">
              {typeName}
            </span>
          </h1>
          {schema?.description && (
            <p className="mt-1 max-w-2xl text-xs leading-relaxed text-muted-foreground">
              {schema.description}
            </p>
          )}
        </div>
        <Link
          href={`/schemas/${encodeURIComponent(typeName)}`}
          prefetch={false}
          className="font-mono text-[11px] font-bold uppercase tracking-widest text-muted-foreground transition-colors hover:text-primary"
        >
          Edit schema →
        </Link>
      </header>
      <DatabaseView key={typeName} typeName={typeName} />
    </div>
  )
}
