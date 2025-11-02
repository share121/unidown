<template>
  <div class="grid min-h-screen place-items-center">
    <div class="w-full max-w-3xl p-2">
      <h1 class="my-8 text-center text-5xl font-bold">Unidown</h1>
      <Form class="mb-4 flex gap-2" @submit="submitHandle">
        <InputText
          class="flex-1"
          fluid
          name="input"
          type="text"
          :disabled="loading"
          placeholder="请输入待解析的 URL"
        />
        <Button type="submit" :loading label="解析" />
      </Form>
      <Fieldset v-if="title" :legend="title">
        <Panel header="合并版" class="mb-2" :collapsed="!mergeUrl">
          <template #icons>
            <Button
              v-if="mergeUrl"
              as="a"
              :href="mergeUrl"
              :download="`${title}.mp4`"
              icon="pi pi-download"
              severity="secondary"
              rounded
              text
              size="small"
            />
          </template>
          <video v-if="mergeUrl" class="w-full" :src="mergeUrl" controls />
        </Panel>
        <Panel header="视频" class="mb-2">
          <template #icons>
            <Button
              v-if="videoUrl"
              as="a"
              :href="videoUrl"
              :download="`${title}.mp4`"
              icon="pi pi-download"
              severity="secondary"
              rounded
              text
              size="small"
            />
          </template>
          <video v-if="videoUrl" class="w-full" :src="videoUrl" controls />
          <ProgressBar
            v-else
            :value="videoProgress"
            :mode="videoProgress ? 'determinate' : 'indeterminate'"
            >{{ videoProgress.toFixed(2) }}%</ProgressBar
          >
        </Panel>
        <Panel header="音频">
          <template #icons>
            <Button
              v-if="audioUrl"
              as="a"
              :href="audioUrl"
              :download="`${title}.mp3`"
              icon="pi pi-download"
              severity="secondary"
              rounded
              text
              size="small"
            />
          </template>
          <audio v-if="audioUrl" class="w-full" :src="audioUrl" controls />
          <ProgressBar
            v-else
            :value="audioProgress"
            :mode="audioProgress ? 'determinate' : 'indeterminate'"
            >{{ audioProgress.toFixed(2) }}%</ProgressBar
          >
        </Panel>
      </Fieldset>
    </div>
  </div>
</template>

<script setup lang="ts">
import type { FormSubmitEvent } from "@primevue/forms";
import { FFmpeg } from "@ffmpeg/ffmpeg";

const loading = ref(false);
const toast = useToast();
const title = ref("");
const videoUrl = ref("");
const audioUrl = ref("");
const mergeUrl = ref("");
const videoBlob = ref<Blob>();
const audioBlob = ref<Blob>();
const videoProgress = ref(0);
const audioProgress = ref(0);
let ffmpeg: FFmpeg | null = null;

onMounted(async () => {
  ffmpeg = new FFmpeg();
  await ffmpeg.load();
  toast.add({ severity: "info", summary: "FFmpeg 加载完成", life: 3000 });
});
watch(videoUrl, (_, oldVal) => {
  if (oldVal) URL.revokeObjectURL(oldVal);
});
watch(audioUrl, (_, oldVal) => {
  if (oldVal) URL.revokeObjectURL(oldVal);
});
watch(mergeUrl, (_, oldVal) => {
  if (oldVal) URL.revokeObjectURL(oldVal);
});
onBeforeUnmount(() => {
  if (videoUrl.value) URL.revokeObjectURL(videoUrl.value);
  if (audioUrl.value) URL.revokeObjectURL(audioUrl.value);
  if (mergeUrl.value) URL.revokeObjectURL(mergeUrl.value);
});

async function submitHandle(e: FormSubmitEvent) {
  if (!e.valid) return;
  loading.value = true;
  title.value = "";
  videoUrl.value = "";
  audioUrl.value = "";
  mergeUrl.value = "";
  videoBlob.value = undefined;
  audioBlob.value = undefined;
  videoProgress.value = 0;
  audioProgress.value = 0;
  try {
    const input = e.values.input as string;
    const res = await $fetch("/api/extract", {
      method: "POST",
      body: { input },
    });
    console.log(res);
    if (isVideoInfo(res)) {
      title.value = res.title;
      const [videoBlobLocal, audioBlobLocal] = await Promise.all([
        fetchWithProgress(res.videoUrl, res.headers, (p, l, t) => {
          console.log("video progress", p, l, t);
          videoProgress.value = p;
        }).then((res) => {
          videoBlob.value = res;
          videoUrl.value = URL.createObjectURL(res);
          return res;
        }),
        res.audioUrl
          ? fetchWithProgress(res.audioUrl, res.headers, (p) => {
              audioProgress.value = p;
            }).then((res) => {
              audioBlob.value = res;
              audioUrl.value = res ? URL.createObjectURL(res) : "";
              return res;
            })
          : undefined,
      ]);
      if (audioBlobLocal) {
        const mergeMedia = await mergeVideo(videoBlobLocal, audioBlobLocal);
        mergeUrl.value = URL.createObjectURL(mergeMedia);
      } else {
        mergeUrl.value = "";
      }
    } else {
      toast.add({
        severity: "error",
        summary: "解析失败",
        detail: res,
        contentStyleClass: "whitespace-pre-wrap",
        closable: true,
      });
    }
  } finally {
    loading.value = false;
  }
}
// eslint-disable-next-line @typescript-eslint/no-explicit-any
function isVideoInfo(obj: any): obj is VideoInfo {
  return "videoUrl" in obj;
}

async function mergeVideo(videoBlob: Blob, audioBlob: Blob): Promise<Blob> {
  if (!ffmpeg) ffmpeg = new FFmpeg();
  await ffmpeg.load();
  const [videoData, audioData] = await Promise.all([
    videoBlob.arrayBuffer(),
    audioBlob.arrayBuffer(),
  ]);
  await ffmpeg.writeFile("input.mp4", new Uint8Array(videoData));
  await ffmpeg.writeFile("input.mp3", new Uint8Array(audioData));
  await ffmpeg.exec([
    "-i",
    "input.mp4",
    "-i",
    "input.mp3",
    "-c",
    "copy",
    "-shortest",
    "output.mp4",
  ]);
  const outputData = await ffmpeg.readFile("output.mp4");
  const outputBlob = new Blob([outputData.slice()], { type: "video/mp4" });
  await ffmpeg.deleteFile("input.mp4");
  await ffmpeg.deleteFile("input.mp3");
  await ffmpeg.deleteFile("output.mp4");
  return outputBlob;
}

async function fetchWithProgress(
  url: string,
  headers: Record<string, string> = {},
  onProgress: (
    progress: number,
    loaded: number,
    total: number,
  ) => void = () => {},
): Promise<Blob> {
  const response = await fetch("https://unidown-fetch.s121.top", {
    method: "POST",
    body: JSON.stringify({
      url,
      headers,
    }),
    headers: {
      "Content-Type": "application/json",
    },
  });
  const contentLength = response.headers.get("content-length");
  const total = contentLength ? parseInt(contentLength, 10) : 0;
  let loaded = 0;
  const reader = response.body?.getReader();
  if (!reader) throw new Error("无法读取响应流");
  const chunks: ArrayBuffer[] = [];
  while (true) {
    const { done, value } = await reader.read();
    if (done) break;
    if (value) {
      chunks.push(value.buffer);
      loaded += value.length;
      const progress = total > 0 ? (loaded / total) * 100 : 0;
      onProgress(progress, loaded, total);
    }
  }
  const blob = new Blob(chunks);
  return blob;
}
</script>
