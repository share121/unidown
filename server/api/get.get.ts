const config = useRuntimeConfig();

export default defineEventHandler(async (event): Promise<unknown> => {
  const { url, ...headers } = getQuery(event);
  const reqHeaders = getHeaders(event);
  console.log("[Proxy]", url, headers);
  if (typeof url !== "string" || url.length === 0) return;
  return fetch(url, {
    headers: { ...config.headers, ...reqHeaders, ...headers },
  });
});
