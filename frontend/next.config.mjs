/** @type {import('next').NextConfig} */
const nextConfig = {
  typescript: {
    ignoreBuildErrors: true,
  },
  images: {
    unoptimized: true,
  },
  async rewrites() {
    return [
      {
        source: "/api/:path*",
        destination: `${process.env.RAG_API_URL || "http://127.0.0.1:4001"}/:path*`,
      },
    ]
  },
}

export default nextConfig
