import {
  CMS_STATIC_STYLES,
  CmsNodeRenderer,
} from "@/components/cms/cms-renderer"
import { fetchCmsTree } from "@/lib/cms"

export const dynamic = "force-dynamic"

export default async function CmsPage({
  params,
}: {
  params: Promise<{ id: string }>
}) {
  const { id } = await params
  const tree = await fetchCmsTree(decodeURIComponent(id))

  return (
    <>
      <style>{CMS_STATIC_STYLES}</style>
      <CmsNodeRenderer node={tree} />
    </>
  )
}
