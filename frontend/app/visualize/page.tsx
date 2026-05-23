import { Suspense } from "react"
import { AppHeader } from "@/components/app-header"
import { GraphView } from "@/components/graph/graph-view"
import { MapView } from "@/components/graph/map-view"
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs"
import { Network, Map as MapIcon } from "lucide-react"

export const metadata = {
  title: "Visualize | RAG Memory & Knowledge",
  description: "Visualize connections between knowledge entries",
}

export default function VisualizePage() {
  return (
    <>
      <AppHeader />
      <main className="h-[calc(100vh-3.5rem)] flex flex-col overflow-hidden">
        <Tabs defaultValue="topology" className="flex-1 flex flex-col h-full relative">
          <div className="absolute top-3 left-4 z-50 pointer-events-none">
            <div className="pointer-events-auto bg-background/60 backdrop-blur-xl border border-primary/10 rounded-full p-1 shadow-2xl">
              <TabsList className="bg-transparent h-9 gap-1">
                <TabsTrigger 
                  value="topology" 
                  className="rounded-full h-7 px-4 text-[10px] font-black uppercase tracking-widest data-[state=active]:bg-primary/20 data-[state=active]:text-primary"
                >
                  <Network className="size-3 mr-2" />
                  Topology
                </TabsTrigger>
                <TabsTrigger 
                  value="map" 
                  className="rounded-full h-7 px-4 text-[10px] font-black uppercase tracking-widest data-[state=active]:bg-primary/20 data-[state=active]:text-primary"
                >
                  <MapIcon className="size-3 mr-2" />
                  Global Map
                </TabsTrigger>
              </TabsList>
            </div>
          </div>

          <TabsContent value="topology" className="flex-1 m-0 p-0 overflow-hidden border-none outline-none">
            <Suspense
              fallback={
                <div className="flex h-full items-center justify-center">
                  <div className="size-8 animate-spin rounded-full border-4 border-muted border-t-primary" />
                </div>
              }
            >
              <GraphView />
            </Suspense>
          </TabsContent>

          <TabsContent value="map" className="flex-1 m-0 p-0 overflow-hidden border-none outline-none">
            <Suspense
              fallback={
                <div className="flex h-full items-center justify-center">
                  <div className="size-8 animate-spin rounded-full border-4 border-muted border-t-primary" />
                </div>
              }
            >
              <MapView />
            </Suspense>
          </TabsContent>
        </Tabs>
      </main>
    </>
  )
}
