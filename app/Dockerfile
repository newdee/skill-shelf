# Build the frontend and serve it via nginx, proxying /api to the server.
FROM node:20-slim AS build
WORKDIR /app
RUN corepack enable
COPY package.json pnpm-lock.yaml* ./
RUN pnpm install --frozen-lockfile || pnpm install
COPY . .
# Same build points at any backend at runtime (Settings page); default to /api.
ENV VITE_API_BASE=/api
RUN pnpm build

FROM nginx:1-alpine
COPY --from=build /app/dist /usr/share/nginx/html
COPY nginx.conf /etc/nginx/conf.d/default.conf
EXPOSE 80
