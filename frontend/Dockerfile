FROM node:22-alpine AS deps

RUN corepack enable

WORKDIR /app

COPY shared/ui/package.json ./shared/ui/package.json
COPY shared/llm/package.json ./shared/llm/package.json
COPY frontend/package.json frontend/pnpm-lock.yaml ./frontend/

WORKDIR /app/frontend

RUN pnpm install --frozen-lockfile --ignore-scripts

FROM node:22-alpine AS builder

RUN corepack enable

WORKDIR /app

COPY --from=deps /app/frontend/node_modules ./frontend/node_modules
COPY shared/ui ./shared/ui
COPY shared/llm ./shared/llm
COPY frontend ./frontend

WORKDIR /app/frontend

ENV NEXT_TELEMETRY_DISABLED=1 \
	CI=true

RUN pnpm build \
	&& pnpm prune --prod --ignore-scripts

FROM node:22-alpine AS runner

RUN addgroup --system --gid 1001 nodejs \
	&& adduser --system --uid 1001 nextjs

WORKDIR /app

ENV NODE_ENV=production \
	NEXT_TELEMETRY_DISABLED=1 \
	PORT=3000 \
	HOSTNAME=0.0.0.0

COPY --from=builder /app/frontend/package.json ./package.json
COPY --from=builder /app/frontend/node_modules ./node_modules
COPY --from=builder /app/frontend/.next ./.next
COPY --from=builder /app/frontend/public ./public
COPY --from=builder /app/frontend/next.config.mjs ./next.config.mjs

USER nextjs

EXPOSE 3000

CMD ["node_modules/.bin/next", "start", "-H", "0.0.0.0", "-p", "3000"]
