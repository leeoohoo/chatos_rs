import { Html, RoundedBox, useTexture } from '@react-three/drei';
import { useFrame, useThree } from '@react-three/fiber';
import { useEffect, useMemo, useRef } from 'react';
import {
  Color,
  DoubleSide,
  type Group,
  MathUtils,
  type Mesh,
  PlaneGeometry,
  PerspectiveCamera,
  RepeatWrapping,
  SRGBColorSpace,
  Vector3,
} from 'three';
import type { TimeMode, ViewMode } from '../../types';

interface CameraAnchor {
  position: [number, number, number];
  target: [number, number, number];
  fov: number;
}

const CAMERA_ANCHORS: Record<ViewMode, CameraAnchor> = {
  room: {
    position: [0, 1.82, 5.35],
    target: [0, 1.55, -0.92],
    fov: 53,
  },
  computer: {
    position: [0, 1.74, 0.25],
    target: [0, 1.74, -0.45],
    fov: 47,
  },
  chat: {
    position: [0, 1.74, 0.25],
    target: [0, 1.74, -0.45],
    fov: 47,
  },
  terminal: {
    position: [0, 1.74, 0.25],
    target: [0, 1.74, -0.45],
    fov: 47,
  },
  remote: {
    position: [0, 1.74, 0.25],
    target: [0, 1.74, -0.45],
    fov: 47,
  },
  archive: {
    position: [-1.82, 1.76, 2.72],
    target: [-5.12, 1.48, -0.55],
    fov: 37,
  },
  project: {
    position: [-0.48, 2.28, 3.62],
    target: [-2.32, 1.72, 0.72],
    fov: 37,
  },
  projection: {
    position: [2.86, 2.32, -0.42],
    target: [5.66, 2.32, -0.42],
    fov: 42,
  },
  phone: {
    position: [0.72, 2.42, 3.55],
    target: [1.62, 1.45, 0.0],
    fov: 38,
  },
};

function useRoomPbrTextures() {
  const { gl } = useThree();
  const [floorColor, floorNormal, floorRoughness, wallColor, wallNormal, wallRoughness] = useTexture([
    '/assets/materials/ambientcg/WoodFloor051/WoodFloor051_1K-JPG_Color.jpg',
    '/assets/materials/ambientcg/WoodFloor051/WoodFloor051_1K-JPG_NormalGL.jpg',
    '/assets/materials/ambientcg/WoodFloor051/WoodFloor051_1K-JPG_Roughness.jpg',
    '/assets/materials/ambientcg/Plaster001/Plaster001_1K-JPG_Color.jpg',
    '/assets/materials/ambientcg/Plaster001/Plaster001_1K-JPG_NormalGL.jpg',
    '/assets/materials/ambientcg/Plaster001/Plaster001_1K-JPG_Roughness.jpg',
  ]);

  useEffect(() => {
    const anisotropy = Math.min(8, gl.capabilities.getMaxAnisotropy());
    floorColor.colorSpace = SRGBColorSpace;
    wallColor.colorSpace = SRGBColorSpace;

    [floorColor, floorNormal, floorRoughness].forEach((texture) => {
      texture.wrapS = RepeatWrapping;
      texture.wrapT = RepeatWrapping;
      texture.repeat.set(3.2, 3.2);
      texture.anisotropy = anisotropy;
      texture.needsUpdate = true;
    });
    [wallColor, wallNormal, wallRoughness].forEach((texture) => {
      texture.wrapS = RepeatWrapping;
      texture.wrapT = RepeatWrapping;
      texture.repeat.set(4.2, 2.4);
      texture.anisotropy = anisotropy;
      texture.needsUpdate = true;
    });
  }, [floorColor, floorNormal, floorRoughness, gl, wallColor, wallNormal, wallRoughness]);

  return { floorColor, floorNormal, floorRoughness, wallColor, wallNormal, wallRoughness };
}

function useRugPbrTextures() {
  const { gl } = useThree();
  const [colorSource, normalSource, roughnessSource] = useTexture([
    '/assets/materials/ambientcg/Fabric036/Fabric036_1K-JPG_Color.jpg',
    '/assets/materials/ambientcg/Fabric036/Fabric036_1K-JPG_NormalGL.jpg',
    '/assets/materials/ambientcg/Fabric036/Fabric036_1K-JPG_Roughness.jpg',
  ]);
  const [color, normal, roughness] = useMemo(
    () => [colorSource.clone(), normalSource.clone(), roughnessSource.clone()],
    [colorSource, normalSource, roughnessSource],
  );

  useEffect(() => {
    const anisotropy = Math.min(8, gl.capabilities.getMaxAnisotropy());
    color.colorSpace = SRGBColorSpace;
    [color, normal, roughness].forEach((texture) => {
      texture.wrapS = RepeatWrapping;
      texture.wrapT = RepeatWrapping;
      texture.repeat.set(2.6, 1.7);
      texture.anisotropy = anisotropy;
      texture.needsUpdate = true;
    });
    return () => {
      color.dispose();
      normal.dispose();
      roughness.dispose();
    };
  }, [color, gl, normal, roughness]);

  return { color, normal, roughness };
}

export function Curtains({ timeMode }: { timeMode: TimeMode }) {
  const [fabricColor, fabricNormal, fabricRoughness] = useTexture([
    '/assets/materials/ambientcg/Fabric036/Fabric036_1K-JPG_Color.jpg',
    '/assets/materials/ambientcg/Fabric036/Fabric036_1K-JPG_NormalGL.jpg',
    '/assets/materials/ambientcg/Fabric036/Fabric036_1K-JPG_Roughness.jpg',
  ]);
  const geometry = useMemo(() => {
    const curtainGeometry = new PlaneGeometry(1.05, 2.62, 28, 1);
    const positions = curtainGeometry.attributes.position;
    for (let index = 0; index < positions.count; index += 1) {
      const x = positions.getX(index);
      const wave = Math.sin((x + 0.52) * Math.PI * 9) * 0.055;
      positions.setZ(index, wave);
    }
    positions.needsUpdate = true;
    curtainGeometry.computeVertexNormals();
    return curtainGeometry;
  }, []);

  useEffect(() => {
    fabricColor.colorSpace = SRGBColorSpace;
    [fabricColor, fabricNormal, fabricRoughness].forEach((texture) => {
      texture.wrapS = RepeatWrapping;
      texture.wrapT = RepeatWrapping;
      texture.repeat.set(1.1, 2.5);
      texture.needsUpdate = true;
    });
    return () => geometry.dispose();
  }, [fabricColor, fabricNormal, fabricRoughness, geometry]);

  return (
    <group>
      <mesh position={[-2.77, 2.55, -3.02]} rotation={[0, 0.08, -0.035]} geometry={geometry} castShadow>
        <meshStandardMaterial
          map={fabricColor}
          normalMap={fabricNormal}
          roughnessMap={fabricRoughness}
          color={timeMode === 'night' ? '#777d88' : '#c9c1b8'}
          roughness={0.96}
          normalScale={[0.28, 0.28]}
          side={DoubleSide}
        />
      </mesh>
      <mesh position={[2.77, 2.55, -3.02]} rotation={[0, -0.08, 0.035]} geometry={geometry} castShadow>
        <meshStandardMaterial
          map={fabricColor}
          normalMap={fabricNormal}
          roughnessMap={fabricRoughness}
          color={timeMode === 'night' ? '#777d88' : '#c9c1b8'}
          roughness={0.96}
          normalScale={[0.28, 0.28]}
          side={DoubleSide}
        />
      </mesh>
      <mesh position={[0, 3.96, -3.02]} rotation={[0, 0, Math.PI / 2]} castShadow>
        <cylinderGeometry args={[0.035, 0.035, 5.85, 20]} />
        <meshStandardMaterial color="#474643" metalness={0.75} roughness={0.28} />
      </mesh>
      {[-2.77, 2.77].map((x) => (
        <mesh key={x} position={[x, 2.08, -2.93]} castShadow>
          <boxGeometry args={[0.22, 0.1, 0.12]} />
          <meshStandardMaterial color="#8f7766" roughness={0.7} />
        </mesh>
      ))}
    </group>
  );
}

export function CameraRig({ view }: { view: ViewMode }) {
  const { camera, invalidate } = useThree();
  const currentTarget = useRef(new Vector3(...CAMERA_ANCHORS.room.target));
  const desiredPosition = useMemo(
    () => new Vector3(...CAMERA_ANCHORS[view].position),
    [view],
  );
  const desiredTarget = useMemo(
    () => new Vector3(...CAMERA_ANCHORS[view].target),
    [view],
  );

  useEffect(() => {
    invalidate();
  }, [desiredPosition, desiredTarget, invalidate, view]);

  useFrame((_, delta) => {
    const computerView = view === 'computer' || view === 'chat' || view === 'terminal' || view === 'remote';
    const factor = 1 - Math.exp(-delta * (computerView ? 12 : 5.2));
    camera.position.lerp(desiredPosition, factor);
    currentTarget.current.lerp(desiredTarget, factor);
    camera.lookAt(currentTarget.current);

    if (camera instanceof PerspectiveCamera) {
      camera.fov = MathUtils.lerp(camera.fov, CAMERA_ANCHORS[view].fov, factor);
      camera.updateProjectionMatrix();
    }
    const moving = camera.position.distanceToSquared(desiredPosition) > 0.000004
      || currentTarget.current.distanceToSquared(desiredTarget) > 0.000004
      || (camera instanceof PerspectiveCamera && Math.abs(camera.fov - CAMERA_ANCHORS[view].fov) > 0.015);
    if (moving) invalidate();
  });

  return null;
}

export function RendererExposure({ timeMode }: { timeMode: TimeMode }) {
  const { gl } = useThree();

  useEffect(() => {
    gl.toneMappingExposure = timeMode === 'night' ? 0.74 : timeMode === 'sunset' ? 0.79 : 0.82;
  }, [gl, timeMode]);

  return null;
}

export function SceneLighting({ timeMode }: { timeMode: TimeMode }) {
  const settings = {
    day: {
      ambient: 0.1,
      hemisphere: 0.28,
      sun: 1.75,
      sunColor: '#fff0bf',
      room: 0.08,
      roomColor: '#ffe7d6',
    },
    sunset: {
      ambient: 0.08,
      hemisphere: 0.18,
      sun: 1.35,
      sunColor: '#ffad8c',
      room: 0.48,
      roomColor: '#ffc788',
    },
    night: {
      ambient: 0.06,
      hemisphere: 0.11,
      sun: 0.28,
      sunColor: '#a8c9ff',
      room: 1.35,
      roomColor: '#ffd095',
    },
  }[timeMode];

  return (
    <>
      <ambientLight intensity={settings.ambient} color={timeMode === 'night' ? '#6f8fc9' : '#f6fbff'} />
      <hemisphereLight
        intensity={settings.hemisphere}
        color={timeMode === 'night' ? '#5f7fb5' : '#d8efff'}
        groundColor={timeMode === 'night' ? '#261f1b' : '#70533f'}
      />
      <directionalLight
        castShadow
        position={[-3.5, 7.5, 2.8]}
        intensity={settings.sun}
        color={settings.sunColor}
        shadow-mapSize-width={1024}
        shadow-mapSize-height={1024}
        shadow-bias={-0.00015}
        shadow-normalBias={0.035}
        shadow-camera-near={0.5}
        shadow-camera-far={18}
        shadow-camera-left={-5}
        shadow-camera-right={5}
        shadow-camera-top={5.5}
        shadow-camera-bottom={-1.5}
      />
      <pointLight
        position={[-2.35, 3.45, -0.4]}
        intensity={settings.room}
        color={settings.roomColor}
        distance={8}
      />
      <pointLight
        position={[0, 2.4, -0.25]}
        intensity={timeMode === 'night' ? 0.82 : 0.12}
        color="#8bc7ff"
        distance={4.5}
      />
      <pointLight
        position={[0, 0.48, 0.9]}
        intensity={timeMode === 'night' ? 0.07 : timeMode === 'sunset' ? 0.15 : 0.09}
        color={timeMode === 'sunset' ? '#ffbd8c' : '#d7b99b'}
        distance={5.5}
      />
      <rectAreaLight
        position={[0, 2.58, -3.02]}
        rotation={[0, Math.PI, 0]}
        width={4.25}
        height={2.15}
        intensity={timeMode === 'night' ? 0.58 : timeMode === 'sunset' ? 2.2 : 2.6}
        color={timeMode === 'night' ? '#88aef0' : timeMode === 'sunset' ? '#ffb28f' : '#d7efff'}
      />
    </>
  );
}

export function RoomShell({ timeMode }: { timeMode: TimeMode }) {
  const textures = useRoomPbrTextures();
  const rugTextures = useRugPbrTextures();
  const wallColor = timeMode === 'night' ? '#333943' : timeMode === 'sunset' ? '#cfc7c2' : '#d8d7d2';
  const sideWallColor = timeMode === 'night' ? '#2c323a' : timeMode === 'sunset' ? '#c6bfba' : '#cecec9';

  return (
    <group>
      <mesh position={[0, -0.08, 0]} receiveShadow>
        <boxGeometry args={[12, 0.2, 7]} />
        <meshPhysicalMaterial
          map={textures.floorColor}
          normalMap={textures.floorNormal}
          roughnessMap={textures.floorRoughness}
          color={timeMode === 'night' ? '#77685d' : '#a98b72'}
          roughness={0.7}
          normalScale={[0.72, 0.72]}
          clearcoat={0.22}
          clearcoatRoughness={0.58}
          envMapIntensity={0.52}
        />
      </mesh>

      <mesh position={[0, 2.38, -3.5]} receiveShadow>
        <boxGeometry args={[12, 4.9, 0.22]} />
        <meshStandardMaterial
          map={textures.wallColor}
          normalMap={textures.wallNormal}
          roughnessMap={textures.wallRoughness}
          color={wallColor}
          roughness={0.92}
          normalScale={[0.22, 0.22]}
        />
      </mesh>

      <mesh position={[-5.95, 2.38, 0]} receiveShadow>
        <boxGeometry args={[0.22, 4.9, 7]} />
        <meshStandardMaterial map={textures.wallColor} normalMap={textures.wallNormal} roughnessMap={textures.wallRoughness} color={sideWallColor} roughness={0.92} normalScale={[0.22, 0.22]} />
      </mesh>
      <mesh position={[5.95, 2.38, 0]} receiveShadow>
        <boxGeometry args={[0.22, 4.9, 7]} />
        <meshStandardMaterial map={textures.wallColor} normalMap={textures.wallNormal} roughnessMap={textures.wallRoughness} color={sideWallColor} roughness={0.92} normalScale={[0.22, 0.22]} />
      </mesh>

      <mesh position={[0, 4.82, 0]} receiveShadow>
        <boxGeometry args={[12, 0.18, 7]} />
        <meshStandardMaterial color={timeMode === 'night' ? '#292f38' : '#e0dfda'} roughness={0.95} />
      </mesh>

      <mesh position={[0, 0.13, -3.38]} receiveShadow>
        <boxGeometry args={[11.7, 0.18, 0.12]} />
        <meshStandardMaterial color={timeMode === 'night' ? '#4a4d51' : '#b9b6af'} roughness={0.78} />
      </mesh>
      <mesh position={[-5.78, 0.13, 0]} receiveShadow>
        <boxGeometry args={[0.12, 0.18, 6.6]} />
        <meshStandardMaterial color={timeMode === 'night' ? '#4a4d51' : '#b9b6af'} roughness={0.78} />
      </mesh>
      <mesh position={[5.78, 0.13, 0]} receiveShadow>
        <boxGeometry args={[0.12, 0.18, 6.6]} />
        <meshStandardMaterial color={timeMode === 'night' ? '#4a4d51' : '#b9b6af'} roughness={0.78} />
      </mesh>

      <RoundedBox
        args={[6.4, 0.035, 3.15]}
        radius={0.075}
        smoothness={4}
        position={[0, 0.055, 1.25]}
        castShadow
        receiveShadow
      >
        <meshStandardMaterial
          map={rugTextures.color}
          normalMap={rugTextures.normal}
          roughnessMap={rugTextures.roughness}
          color={timeMode === 'night' ? '#56606b' : timeMode === 'sunset' ? '#9b8378' : '#a89d8f'}
          roughness={0.94}
          normalScale={[0.32, 0.32]}
        />
      </RoundedBox>

    </group>
  );
}
