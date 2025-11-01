export interface VideoInfo {
  title: string;
  videoUrl: string;
  audioUrl?: string;
  headers: Record<string, string>;
}
export type MaybePromise<T> = T | Promise<T>;
export type ExtractFn = (
  input: string,
  context: ExtractContext,
) => MaybePromise<VideoInfo | undefined | null>;
export interface Extractor {
  name: string;
  extract: ExtractFn;
}
export interface ExtractContext {
  requestUrl: URL;
}
export const extractors: Promise<Extractor>[] = [import("./bilibili")];
