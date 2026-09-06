<script setup lang="ts">
interface Props {
  curvature?: number;
  reverse?: boolean;
  pathColor?: string;
  pathWidth?: number;
  pathOpacity?: number;
  gradientStartColor?: string;
  gradientStopColor?: string;
  delay?: number;
  duration?: number;
  startXOffset?: number;
  startYOffset?: number;
  endXOffset?: number;
  endYOffset?: number;
}

const props = withDefaults(defineProps<Props>(), {
  curvature: 75,
  reverse: false,
  pathColor: "gray",
  pathWidth: 2,
  pathOpacity: 0.2,
  gradientStartColor: "#ffaa40",
  gradientStopColor: "#9c40ff",
  delay: 0,
  duration: 4,
  startXOffset: 0,
  startYOffset: 0,
  endXOffset: 0,
  endYOffset: 10,
});

const containerRef = useTemplateRef("containerRef");
const div1Ref = useTemplateRef("div1Ref");
const div2Ref = useTemplateRef("div2Ref");
const div3Ref = useTemplateRef("div3Ref");
const div4Ref = useTemplateRef("div4Ref");
const div5Ref = useTemplateRef("div5Ref");
const div6Ref = useTemplateRef("div6Ref");
const div7Ref = useTemplateRef("div7Ref");
</script>

<template>
  <div
    ref="containerRef"
    class="bg-background relative flex h-[500px] w-full items-center justify-center overflow-hidden rounded-lg border p-10 md:shadow-xl"
  >
    <div
      class="flex size-full max-h-[200px] max-w-lg flex-col items-stretch justify-between gap-10"
    >
      <div class="flex flex-row items-center justify-between">
        <div
          ref="div1Ref"
          class="z-10 flex size-12 items-center justify-center rounded-full border-2 bg-white p-2 shadow-[0_0_20px_-12px_rgba(0,0,0,0.8)] dark:text-black"
        >
          <Icon
            name="devicon:googlecloud"
            size="24"
          />
        </div>
        <div
          ref="div5Ref"
          class="z-10 flex size-12 items-center justify-center rounded-full border-2 bg-white p-2 shadow-[0_0_20px_-12px_rgba(0,0,0,0.8)]"
        >
          <Icon
            name="logos:google-drive"
            size="24"
          />
        </div>
      </div>
      <div class="flex flex-row items-center justify-between">
        <div
          ref="div2Ref"
          class="z-10 flex size-12 items-center justify-center rounded-full border-2 bg-white p-2 shadow-[0_0_20px_-12px_rgba(0,0,0,0.8)] dark:text-black"
        >
          <Icon
            name="logos:notion-icon"
            size="24"
          />
        </div>
        <div
          ref="div4Ref"
          class="z-10 flex size-16 items-center justify-center rounded-full border-2 bg-white p-2 text-black shadow-[0_0_20px_-12px_rgba(0,0,0,0.8)]"
        >
          <Icon
            name="simple-icons:openai"
            size="30"
          />
        </div>
        <div
          ref="div6Ref"
          class="z-10 flex size-12 items-center justify-center rounded-full border-2 bg-white p-2 shadow-[0_0_20px_-12px_rgba(0,0,0,0.8)]"
        >
          <Icon
            name="logos:google-gmail"
            size="24"
          />
        </div>
      </div>
      <div class="flex flex-row items-center justify-between">
        <div
          ref="div3Ref"
          class="z-10 flex size-12 items-center justify-center rounded-full border-2 bg-white p-2 shadow-[0_0_20px_-12px_rgba(0,0,0,0.8)] dark:text-black"
        >
          <Icon
            name="logos:whatsapp-icon"
            size="24"
          />
        </div>
        <div
          ref="div7Ref"
          class="z-10 flex size-12 items-center justify-center rounded-full border-2 bg-white p-2 shadow-[0_0_20px_-12px_rgba(0,0,0,0.8)]"
        >
          <Icon
            name="logos:messenger"
            size="24"
          />
        </div>
      </div>
    </div>

    <AnimatedBeam
      v-bind="props"
      :container-ref="containerRef"
      :from-ref="div1Ref"
      :to-ref="div4Ref"
      :curvature="-props.curvature"
      :end-y-offset="-props.endYOffset"
    />
    <AnimatedBeam
      v-bind="props"
      :container-ref="containerRef"
      :from-ref="div2Ref"
      :to-ref="div4Ref"
      :curvature="0"
      :end-y-offset="0"
    />
    <AnimatedBeam
      v-bind="props"
      :container-ref="containerRef"
      :from-ref="div3Ref"
      :to-ref="div4Ref"
      :curvature="props.curvature"
      :end-y-offset="props.endYOffset"
    />
    <AnimatedBeam
      v-bind="props"
      :container-ref="containerRef"
      :from-ref="div5Ref"
      :to-ref="div4Ref"
      :curvature="-props.curvature"
      :end-y-offset="-props.endYOffset"
    />
    <AnimatedBeam
      v-bind="props"
      :container-ref="containerRef"
      :from-ref="div6Ref"
      :to-ref="div4Ref"
      :curvature="0"
      :end-y-offset="0"
    />
    <AnimatedBeam
      v-bind="props"
      :container-ref="containerRef"
      :from-ref="div7Ref"
      :to-ref="div4Ref"
      :curvature="props.curvature"
      :end-y-offset="props.endYOffset"
    />
  </div>
</template>
