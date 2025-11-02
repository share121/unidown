import type { ExtractFn } from "./extractors";

export const name = "bilibili";
export const extract: ExtractFn = async (input) => {
  const bvid = extractBvid(input);
  if (!bvid) return null;
  const { cid, title } = await getInfo(bvid);
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const playinfo: any = await $fetch(
    `https://api.bilibili.com/x/player/playurl?bvid=${bvid}&cid=${cid}&qn=80&fnval=4048&fourk=1&try_look=1`,
    {
      cache: "no-cache",
    },
  );
  const videoUrl = playinfo.data.dash.video[0].baseUrl;
  const audioUrl = playinfo.data.dash.audio[0].baseUrl;
  console.log(`[Extractor][bilibili][VideoURL]`, videoUrl);
  console.log(`[Extractor][bilibili][AudioURL]`, audioUrl);
  return {
    title,
    videoUrl,
    audioUrl,
    headers: { referer: "https://www.bilibili.com" },
  };
};

async function getInfo(bvid: string) {
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const resp: any = await $fetch(
    `https://api.bilibili.com/x/player/pagelist?bvid=${bvid}`,
    {
      cache: "no-cache",
    },
  );
  return {
    title: resp.data[0].part as string,
    cid: resp.data[0].cid as number,
  };
}
function extractBvid(url: string) {
  return url.match(/\bBV\w{10}\b/i)?.[0];
}
