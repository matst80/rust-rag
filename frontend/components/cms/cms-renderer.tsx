import React from "react"
import ReactMarkdown from "react-markdown"
import type { CmsTreeChild, CmsTreeNode, Entry } from "@/lib/api/types"

export const CMS_STRUCTURAL_RELATIONS = new Set(["contains", "part_of"])

function dataString(
  value: unknown,
  fallback = "",
): string {
  return typeof value === "string" ? value : fallback
}

function entryTitle(entry: Entry): string {
  const explicit =
    (typeof entry.data?.title === "string" && entry.data.title) ||
    (typeof entry.metadata?.title === "string" && entry.metadata.title) ||
    entry.id
  return explicit
}

function renderChildren(children: CmsTreeChild[]) {
  return children.map(({ edge, node }) => (
    <CmsNodeRenderer
      key={`${edge.id}:${node.entry.id}`}
      node={node}
    />
  ))
}

function CmsPage({ node }: { node: CmsTreeNode }) {
  const title = dataString(node.entry.data?.title, entryTitle(node.entry))
  const description = dataString(node.entry.data?.description)

  return (
    <main className="cms-page">
      <header className="cms-page-header">
        <p className="cms-kicker">CMS Page</p>
        <h1>{title}</h1>
        {description ? <p className="cms-page-description">{description}</p> : null}
      </header>
      <div className="cms-page-body">{renderChildren(node.children)}</div>
    </main>
  )
}

function CmsSwiper({ node }: { node: CmsTreeNode }) {
  const title = dataString(node.entry.data?.title)

  return (
    <section className="cms-block cms-swiper">
      {title ? <h2>{title}</h2> : null}
      <div className="cms-swiper-track">{renderChildren(node.children)}</div>
    </section>
  )
}

function CmsImage({ node }: { node: CmsTreeNode }) {
  const src =
    dataString(node.entry.data?.src) ||
    dataString(node.entry.data?.url) ||
    dataString(node.entry.metadata?.source_file)
  const alt = dataString(node.entry.data?.alt, entryTitle(node.entry))
  const caption = dataString(node.entry.data?.caption)

  if (!src) {
    return (
      <section className="cms-block cms-empty">
        <strong>Image missing source</strong>
      </section>
    )
  }

  return (
    <figure className="cms-block cms-image">
      <img src={src} alt={alt} />
      {caption ? <figcaption>{caption}</figcaption> : null}
    </figure>
  )
}

function CmsMarkdown({ node }: { node: CmsTreeNode }) {
  const content =
    dataString(node.entry.data?.markdown) ||
    dataString(node.entry.data?.body) ||
    node.entry.text

  return (
    <section className="cms-block cms-markdown">
      <ReactMarkdown>{content}</ReactMarkdown>
    </section>
  )
}

function CmsSection({ node }: { node: CmsTreeNode }) {
  const title = dataString(node.entry.data?.title, entryTitle(node.entry))

  return (
    <section className="cms-block cms-section">
      <h2>{title}</h2>
      <div className="cms-section-body">{renderChildren(node.children)}</div>
    </section>
  )
}

function CmsFallback({ node }: { node: CmsTreeNode }) {
  return (
    <section className="cms-block cms-fallback">
      <h2>{entryTitle(node.entry)}</h2>
      {node.entry.text ? <ReactMarkdown>{node.entry.text}</ReactMarkdown> : null}
      {node.children.length > 0 ? (
        <div className="cms-section-body">{renderChildren(node.children)}</div>
      ) : null}
    </section>
  )
}

export function CmsNodeRenderer({ node }: { node: CmsTreeNode }) {
  switch (node.entry.type) {
    case "cms_page":
      return <CmsPage node={node} />
    case "cms_swiper":
      return <CmsSwiper node={node} />
    case "cms_image":
      return <CmsImage node={node} />
    case "cms_markdown":
      return <CmsMarkdown node={node} />
    case "cms_section":
      return <CmsSection node={node} />
    default:
      return <CmsFallback node={node} />
  }
}

export const CMS_STATIC_STYLES = `
  :root { color-scheme: light dark; }
  * { box-sizing: border-box; }
  body {
    margin: 0;
    font-family: Inter, ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
    background: #0b1020;
    color: #e6edf3;
  }
  a { color: inherit; }
  img { display: block; max-width: 100%; height: auto; border-radius: 18px; }
  .cms-page { max-width: 1120px; margin: 0 auto; padding: 56px 24px 96px; }
  .cms-page-header { margin-bottom: 32px; }
  .cms-page-header h1, .cms-block h2 { margin: 0 0 12px; line-height: 1.05; }
  .cms-page-header h1 { font-size: clamp(2.4rem, 5vw, 4.5rem); }
  .cms-page-description { margin: 0; max-width: 70ch; color: #9fb0c3; font-size: 1.05rem; }
  .cms-kicker { margin: 0 0 10px; text-transform: uppercase; letter-spacing: .14em; color: #8cc4ff; font-size: .72rem; font-weight: 700; }
  .cms-page-body, .cms-section-body { display: grid; gap: 24px; }
  .cms-block {
    background: rgba(15, 23, 42, .72);
    border: 1px solid rgba(148, 163, 184, .18);
    border-radius: 24px;
    padding: 24px;
    backdrop-filter: blur(16px);
  }
  .cms-markdown { line-height: 1.7; }
  .cms-markdown :is(h1,h2,h3,h4,p,ul,ol,blockquote) { margin-top: 0; }
  .cms-markdown code {
    padding: .15rem .35rem;
    border-radius: 8px;
    background: rgba(148, 163, 184, .14);
  }
  .cms-markdown pre {
    overflow: auto;
    padding: 16px;
    border-radius: 18px;
    background: rgba(2, 6, 23, .9);
  }
  .cms-swiper-track {
    display: grid;
    grid-auto-flow: column;
    grid-auto-columns: minmax(280px, 82%);
    gap: 16px;
    overflow-x: auto;
    padding-bottom: 8px;
    scroll-snap-type: x mandatory;
  }
  .cms-swiper-track > * { scroll-snap-align: start; }
  .cms-image figcaption {
    margin-top: 12px;
    color: #9fb0c3;
    font-size: .92rem;
  }
  .cms-empty {
    border-style: dashed;
    color: #fda4af;
  }
`
