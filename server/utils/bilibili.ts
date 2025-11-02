import type { ExtractFn } from "./extractors";

const config = useRuntimeConfig();
export const name = "\u0062\u0069\u006c\u0069\u0062\u0069\u006c\u0069";
export const extract: ExtractFn = async (input) => {
  const bvid = extractBvid(input);
  if (!bvid) return null;
  const { cid, title } = await getInfo(bvid);
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const playinfo: any = await $fetch(
    `https://api.${name}.com/x/player/playurl?qn=80&fnval=4048&fourk=1&try_look=1`,
    {
      headers: config.headers,
      query: { bvid, cid },
    },
  );
  const videoUrl = playinfo.data.dash.video[0].baseUrl;
  const audioUrl = playinfo.data.dash.audio[0].baseUrl;
  console.log(`[Extractor][${name}][VideoURL]`, videoUrl);
  console.log(`[Extractor][${name}][AudioURL]`, audioUrl);
  return {
    title,
    videoUrl,
    audioUrl,
    headers: { referer: `https://www.${name}.com` },
  };
};

async function getInfo(bvid: string) {
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const resp: any = await $fetch(`https://api.${name}.com/x/player/pagelist`, {
    headers: config.headers,
    query: { bvid },
  });
  return {
    title: resp.data[0].part as string,
    cid: resp.data[0].cid as number,
  };
}
function extractBvid(url: string) {
  return url.match(/\bBV\w{10}\b/i)?.[0];
}
