const config = useRuntimeConfig();

export default defineEventHandler(async (event): Promise<unknown> => {
  const { url, headers } = await readBody(event);
  console.log("[Proxy]", url, headers);
  return fetch(url, {
    headers: { ...config.app.headers, ...headers },
  });
});
