"use client"

import * as React from "react"
import * as THREE from "three"
import { OrbitControls } from "three/examples/jsm/controls/OrbitControls.js"
import type { MapPoint } from "@/lib/api/types"

interface Props {
  points: MapPoint[]
  colors: string[]
  focusedCluster: number | null
  onPick: (p: MapPoint) => void
  onHover: (p: MapPoint | null, screen: { x: number; y: number } | null) => void
}

function computeBounds(points: MapPoint[]) {
  let minX = Infinity, maxX = -Infinity
  let minY = Infinity, maxY = -Infinity
  let minZ = Infinity, maxZ = -Infinity
  for (const p of points) {
    if (p.x < minX) minX = p.x; if (p.x > maxX) maxX = p.x
    if (p.y < minY) minY = p.y; if (p.y > maxY) maxY = p.y
    const z = p.z ?? 0
    if (z < minZ) minZ = z; if (z > maxZ) maxZ = z
  }
  return { minX, maxX, minY, maxY, minZ, maxZ }
}

export function MapView3D({ points, colors, focusedCluster, onPick, onHover }: Props) {
  const mountRef = React.useRef<HTMLDivElement>(null)
  const stateRef = React.useRef<{
    renderer?: THREE.WebGLRenderer
    scene?: THREE.Scene
    camera?: THREE.PerspectiveCamera
    controls?: OrbitControls
    mesh?: THREE.InstancedMesh
    raycaster?: THREE.Raycaster
    positions?: THREE.Vector3[]
    pointMap?: MapPoint[]
    hoverIdx: number | null
    focused: number | null
    colors: string[]
    onPick: (p: MapPoint) => void
    onHover: (p: MapPoint | null, s: { x: number; y: number } | null) => void
    raf?: number
  }>({ hoverIdx: null, focused: null, colors, onPick, onHover })

  // Keep callbacks + focus in ref so animation loop sees latest without re-init
  React.useEffect(() => {
    stateRef.current.onPick = onPick
    stateRef.current.onHover = onHover
    stateRef.current.colors = colors
  }, [onPick, onHover, colors])

  React.useEffect(() => {
    stateRef.current.focused = focusedCluster
    paintColors()
  }, [focusedCluster])

  function paintColors() {
    const s = stateRef.current
    if (!s.mesh || !s.pointMap) return
    const color = new THREE.Color()
    s.pointMap.forEach((p, i) => {
      const isHover = s.hoverIdx === i
      const inFocus = s.focused === null || s.focused === p.cluster
      color.set(s.colors[p.cluster % s.colors.length])
      if (!inFocus) color.multiplyScalar(0.15)
      else if (s.hoverIdx !== null && !isHover) color.multiplyScalar(0.4)
      s.mesh!.setColorAt(i, color)
    })
    if (s.mesh.instanceColor) s.mesh.instanceColor.needsUpdate = true
  }

  function paintMatrices() {
    const s = stateRef.current
    if (!s.mesh || !s.positions || !s.pointMap) return
    const dummy = new THREE.Object3D()
    s.positions.forEach((pos, i) => {
      dummy.position.copy(pos)
      const isHover = s.hoverIdx === i
      dummy.scale.setScalar(isHover ? 1.8 : 1)
      dummy.updateMatrix()
      s.mesh!.setMatrixAt(i, dummy.matrix)
    })
    s.mesh.instanceMatrix.needsUpdate = true
  }

  // Init scene once
  React.useEffect(() => {
    const mount = mountRef.current
    if (!mount) return

    const width = mount.clientWidth
    const height = mount.clientHeight

    const scene = new THREE.Scene()
    const camera = new THREE.PerspectiveCamera(50, width / height, 0.1, 100)
    camera.position.set(6, 5, 8)

    const renderer = new THREE.WebGLRenderer({ antialias: true, alpha: true })
    renderer.setPixelRatio(window.devicePixelRatio)
    renderer.setSize(width, height)
    renderer.setClearColor(0x000000, 0)
    mount.appendChild(renderer.domElement)

    const ambient = new THREE.AmbientLight(0xffffff, 1.2)
    scene.add(ambient)
    const dir1 = new THREE.DirectionalLight(0xffffff, 1.5)
    dir1.position.set(5, 10, 7)
    scene.add(dir1)
    const dir2 = new THREE.DirectionalLight(0xffffff, 0.8)
    dir2.position.set(-5, -3, -5)
    scene.add(dir2)

    const grid = new THREE.GridHelper(10, 10, 0x334155, 0x1e293b)
    grid.position.y = -3.2
    scene.add(grid)

    const controls = new OrbitControls(camera, renderer.domElement)
    controls.enableDamping = true
    controls.dampingFactor = 0.1

    const raycaster = new THREE.Raycaster()

    Object.assign(stateRef.current, { renderer, scene, camera, controls, raycaster })

    // Pointer handling
    const pointer = new THREE.Vector2()
    const onPointerMove = (e: PointerEvent) => {
      const s = stateRef.current
      if (!s.mesh || !s.camera || !s.raycaster) return
      const rect = renderer.domElement.getBoundingClientRect()
      pointer.x = ((e.clientX - rect.left) / rect.width) * 2 - 1
      pointer.y = -((e.clientY - rect.top) / rect.height) * 2 + 1
      s.raycaster.setFromCamera(pointer, s.camera)
      const hits = s.raycaster.intersectObject(s.mesh, false)
      const idx = hits.length > 0 ? hits[0].instanceId ?? null : null
      if (idx !== s.hoverIdx) {
        s.hoverIdx = idx
        paintMatrices()
        paintColors()
      }
      if (idx !== null && s.pointMap) {
        const p = s.pointMap[idx]
        s.onHover(p, { x: e.clientX - rect.left, y: e.clientY - rect.top })
      } else {
        s.onHover(null, null)
      }
    }
    const onLeave = () => {
      const s = stateRef.current
      if (s.hoverIdx !== null) {
        s.hoverIdx = null
        paintMatrices()
        paintColors()
      }
      s.onHover(null, null)
    }
    const onClick = () => {
      const s = stateRef.current
      if (s.hoverIdx !== null && s.pointMap) s.onPick(s.pointMap[s.hoverIdx])
    }
    renderer.domElement.addEventListener("pointermove", onPointerMove)
    renderer.domElement.addEventListener("pointerleave", onLeave)
    renderer.domElement.addEventListener("click", onClick)

    // Resize
    const ro = new ResizeObserver(() => {
      const w = mount.clientWidth
      const h = mount.clientHeight
      renderer.setSize(w, h)
      camera.aspect = w / h
      camera.updateProjectionMatrix()
    })
    ro.observe(mount)

    // Loop
    const tick = () => {
      controls.update()
      renderer.render(scene, camera)
      stateRef.current.raf = requestAnimationFrame(tick)
    }
    tick()

    return () => {
      if (stateRef.current.raf) cancelAnimationFrame(stateRef.current.raf)
      ro.disconnect()
      renderer.domElement.removeEventListener("pointermove", onPointerMove)
      renderer.domElement.removeEventListener("pointerleave", onLeave)
      renderer.domElement.removeEventListener("click", onClick)
      controls.dispose()
      renderer.dispose()
      if (renderer.domElement.parentNode) renderer.domElement.parentNode.removeChild(renderer.domElement)
    }
  }, [])

  // Rebuild instanced mesh when points change
  React.useEffect(() => {
    const s = stateRef.current
    if (!s.scene) return
    if (s.mesh) {
      s.scene.remove(s.mesh)
      s.mesh.geometry.dispose()
      ;(s.mesh.material as THREE.Material).dispose()
      s.mesh = undefined
    }
    if (points.length === 0) {
      s.pointMap = []
      s.positions = []
      return
    }
    const b = computeBounds(points)
    const rx = b.maxX - b.minX || 1
    const ry = b.maxY - b.minY || 1
    const rz = b.maxZ - b.minZ || 1
    const scale = 6
    const positions = points.map(p => new THREE.Vector3(
      ((p.x - b.minX) / rx - 0.5) * scale,
      ((p.y - b.minY) / ry - 0.5) * scale,
      (((p.z ?? 0) - b.minZ) / rz - 0.5) * scale,
    ))

    const geom = new THREE.SphereGeometry(0.08, 12, 12)
    const mat = new THREE.MeshStandardMaterial({
      color: 0xffffff,
      roughness: 0.4,
      metalness: 0.0,
      emissive: 0x000000,
    })
    const mesh = new THREE.InstancedMesh(geom, mat, points.length)
    mesh.instanceMatrix.setUsage(THREE.DynamicDrawUsage)
    mesh.instanceColor = new THREE.InstancedBufferAttribute(new Float32Array(points.length * 3), 3)
    s.mesh = mesh
    s.positions = positions
    s.pointMap = points
    s.hoverIdx = null
    s.scene.add(mesh)
    paintMatrices()
    paintColors()
  }, [points])

  return <div ref={mountRef} className="absolute inset-0" style={{ touchAction: "none" }} />
}
