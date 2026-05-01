/** @type {import('next').NextConfig} */
const RAG_API_URL = process.env.RAG_API_URL || 'http://localhost:4001';
console.log('Next.js rewrites using RAG_API_URL:', RAG_API_URL);

const nextConfig = {
  allowedDevOrigins: ["127.0.0.1", "10.10.11.135", "rag.k6n.net"],
  typescript: {
    ignoreBuildErrors: true,
  },
  images: {
    unoptimized: true,
  },
  async rewrites() {
    return [
      {
        source: '/api/:path*',
        destination: `${RAG_API_URL}/api/:path*`,
      },
      {
        source: '/admin/:path*',
        destination: `${RAG_API_URL}/admin/:path*`,
      },
      {
        source: '/assets/:path*',
        destination: `${RAG_API_URL}/assets/:path*`,
      },
    ]
  },
}

export default nextConfig
