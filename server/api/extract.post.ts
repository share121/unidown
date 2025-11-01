import { extractors } from "../utils/extractors";

export interface ExtractError {
  extractor: string;
  message: string;
}

export default defineEventHandler(async (event) => {
  const { input } = await readBody(event);
  const requestUrl = getRequestURL(event);
  const error: ExtractError[] = [];
  for (const promise of extractors) {
    const extractor = await promise;
    try {
      const res = await extractor.extract(input, { requestUrl });
      if (res) return res;
    } catch (e) {
      console.warn(`[Extractor Error][${extractor.name}]`, e);
      let message = "Unknown error";
      if (e instanceof Error) message = e.message;
      else if (typeof e === "string") message = e;
      error.push({
        extractor: extractor.name,
        message,
      });
    }
  }
  return { error };
});
