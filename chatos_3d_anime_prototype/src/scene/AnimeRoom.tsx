import { ContactShadows, Environment, Html, RoundedBox, useGLTF, useTexture } from '@react-three/drei';
import { useFrame, useThree } from '@react-three/fiber';
import { Bot, LockKeyhole, MonitorUp, TerminalSquare, UserRound } from 'lucide-react';
import { type ReactNode, useEffect, useMemo, useRef, useState } from 'react';
import {
  Color,
  DoubleSide,
  type Group,
  MathUtils,
  type Mesh,
  type Object3D,
  PlaneGeometry,
  PerspectiveCamera,
  RepeatWrapping,
  SRGBColorSpace,
  Vector3,
} from 'three';
import type { DemoProject, DemoTask, TimeMode, ViewMode } from '../types';
import { Cat } from './Cat';
import { WindowView } from './WindowView';

interface RealisticRoomProps {
  view: ViewMode;
  timeMode: TimeMode;
  projects: DemoProject[];
  tasks: DemoTask[];
  computerLocked: boolean;
  computerScreenCovered: boolean;
  computerContent: ReactNode;
  taskWallContent: ReactNode;
  onViewChange: (view: ViewMode) => void;
  onComputerLock: () => void;
  onProjectSelect: (project: DemoProject) => void;
  onCatPet: () => void;
}

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

function RealModel({
  url,
  position,
  rotation,
  scale = 1,
}: {
  url: string;
  position?: [number, number, number];
  rotation?: [number, number, number];
  scale?: number | [number, number, number];
}) {
  const { scene } = useGLTF(url);
  const model = useMemo(() => scene.clone(true), [scene]);

  useEffect(() => {
    model.traverse((child: Object3D) => {
      const mesh = child as Mesh;
      if (!mesh.isMesh) return;
      mesh.castShadow = true;
      mesh.receiveShadow = true;
    });
  }, [model]);

  return <primitive object={model} position={position} rotation={rotation} scale={scale} />;
}

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

function Curtains({ timeMode }: { timeMode: TimeMode }) {
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

function CameraRig({ view }: { view: ViewMode }) {
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

function RendererExposure({ timeMode }: { timeMode: TimeMode }) {
  const { gl } = useThree();

  useEffect(() => {
    gl.toneMappingExposure = timeMode === 'night' ? 0.74 : timeMode === 'sunset' ? 0.79 : 0.82;
  }, [gl, timeMode]);

  return null;
}

function SceneLighting({ timeMode }: { timeMode: TimeMode }) {
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

function RoomShell({ timeMode }: { timeMode: TimeMode }) {
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

function DeskLamp({ timeMode }: { timeMode: TimeMode }) {
  return (
    <group>
      <RealModel
        url="/assets/models/polyhaven/desk_lamp_arm_01/desk_lamp_arm_01.gltf"
        position={[-1.72, 1.08, -0.2]}
        rotation={[0, -0.35, 0]}
        scale={1.05}
      />
      <spotLight
        position={[-1.82, 1.92, -0.08]}
        target-position={[-1.4, 1.02, 0.24]}
        intensity={timeMode === 'night' ? 3.2 : 0.22}
        angle={0.55}
        penumbra={0.72}
        distance={4}
        color="#ffd0a0"
      />
    </group>
  );
}

const KEYBOARD_ROWS = [14, 14, 13, 12];

function KeyboardAndMouse() {
  return (
    <group>
      <RoundedBox
        args={[1.78, 0.014, 0.68]}
        radius={0.055}
        smoothness={3}
        position={[0.08, 1.058, 0.67]}
        receiveShadow
      >
        <meshStandardMaterial color="#625347" roughness={0.88} />
      </RoundedBox>

      <group position={[-0.12, 1.12, 0.64]} rotation={[-0.18, 0, 0]}>
        <RoundedBox args={[1.3, 0.055, 0.5]} radius={0.035} smoothness={3} castShadow>
          <meshStandardMaterial color="#d2d7d6" metalness={0.08} roughness={0.42} />
        </RoundedBox>
        {KEYBOARD_ROWS.map((count, row) => {
          const step = 0.085;
          const start = -((count - 1) * step) / 2;
          return Array.from({ length: count }).map((_, column) => (
            <RoundedBox
              key={`${row}-${column}`}
              args={[0.068, 0.026, 0.072]}
              radius={0.009}
              smoothness={2}
              position={[start + column * step, 0.042, -0.15 + row * 0.094]}
              castShadow
            >
              <meshStandardMaterial
                color={row === 0 ? '#e2e6e5' : '#d3d9d9'}
                roughness={0.56}
              />
            </RoundedBox>
          ));
        })}
        <RoundedBox args={[0.48, 0.026, 0.072]} radius={0.009} smoothness={2} position={[0, 0.042, 0.226]} castShadow>
          <meshStandardMaterial color="#d3d9d9" roughness={0.56} />
        </RoundedBox>
        {[-0.54, -0.45, -0.36, 0.36, 0.45, 0.54].map((x) => (
          <RoundedBox key={x} args={[0.068, 0.026, 0.072]} radius={0.009} smoothness={2} position={[x, 0.042, 0.226]} castShadow>
            <meshStandardMaterial color="#c8d0d1" roughness={0.58} />
          </RoundedBox>
        ))}
      </group>

      <group position={[0.77, 1.14, 0.66]} rotation={[-0.08, -0.08, 0]}>
        <mesh rotation={[Math.PI / 2, 0, 0]} scale={[1, 1, 0.45]} castShadow>
          <capsuleGeometry args={[0.11, 0.12, 8, 24]} />
          <meshStandardMaterial color="#30363a" metalness={0.16} roughness={0.38} />
        </mesh>
        <mesh position={[0, 0.048, -0.035]}>
          <boxGeometry args={[0.012, 0.008, 0.13]} />
          <meshStandardMaterial color="#778286" roughness={0.42} />
        </mesh>
        <mesh position={[0, 0.059, -0.085]}>
          <cylinderGeometry args={[0.018, 0.018, 0.025, 14]} />
          <meshStandardMaterial color="#4c565a" roughness={0.72} />
        </mesh>
      </group>
    </group>
  );
}

function Computer({
  onDesktop,
  onChat,
  onTerminal,
  onRemote,
  onLock,
  focused,
  locked,
  screenCovered,
  content,
  timeMode,
}: {
  onDesktop: () => void;
  onChat: () => void;
  onTerminal: () => void;
  onRemote: () => void;
  onLock: () => void;
  focused: boolean;
  locked: boolean;
  screenCovered: boolean;
  content: ReactNode;
  timeMode: TimeMode;
}) {
  const [hovered, setHovered] = useState(false);
  const greeting = timeMode === 'day'
    ? '上午好，指挥官'
    : timeMode === 'sunset'
      ? '傍晚好，指挥官'
      : '晚上好，指挥官';

  return (
    <group position={[0, 0, -0.35]}>
      <group
        onClick={(event) => {
          event.stopPropagation();
          onDesktop();
        }}
        onPointerEnter={(event) => {
          event.stopPropagation();
          setHovered(true);
          document.body.style.cursor = 'pointer';
        }}
        onPointerLeave={() => {
          setHovered(false);
          document.body.style.cursor = 'default';
        }}
      >
        <RoundedBox args={[1.62, 1.02, 0.11]} radius={0.055} smoothness={4} position={[0, 1.74, -0.45]} castShadow>
          <meshStandardMaterial color={hovered ? '#26313a' : '#171b1f'} metalness={0.58} roughness={0.26} />
        </RoundedBox>
        <mesh position={[0, 1.74, -0.382]}>
          <planeGeometry args={[1.47, 0.84]} />
          <meshPhysicalMaterial
            color="#070d13"
            emissive={hovered ? '#17324a' : '#0b1824'}
            emissiveIntensity={hovered ? 0.42 : 0.24}
            roughness={0.12}
            clearcoat={0.85}
            clearcoatRoughness={0.08}
            toneMapped={false}
          />
        </mesh>
        <mesh position={[0, 2.218, -0.315]}>
          <sphereGeometry args={[0.012, 12, 8]} />
          <meshStandardMaterial color="#090b0c" roughness={0.22} />
        </mesh>
        <mesh position={[0.72, 1.258, -0.315]}>
          <sphereGeometry args={[0.009, 10, 7]} />
          <meshBasicMaterial color="#73d9aa" />
        </mesh>
        <Html
          transform
          center
          distanceFactor={1.55}
          position={[0, 1.74, -0.28]}
          style={{
            display: screenCovered ? 'none' : 'block',
            pointerEvents: screenCovered ? 'none' : 'auto',
            visibility: screenCovered ? 'hidden' : 'visible',
          }}
        >
          <div className={screenCovered ? 'monitor-html-stage is-covered' : 'monitor-html-stage'} aria-hidden={screenCovered}>
          {focused ? (
            <div className="monitor-app-fullscreen" onPointerDown={(event) => event.stopPropagation()} onClick={(event) => event.stopPropagation()}>
              {content}
            </div>
          ) : locked ? (
              <div
                className={`monitor-login-preview is-${timeMode}`}
                onPointerDown={(event) => event.stopPropagation()}
                onClick={(event) => {
                  event.stopPropagation();
                  onDesktop();
                }}
              >
                <div className="monitor-login-preview__time">
                  <b>{timeMode === 'day' ? '09:41' : timeMode === 'sunset' ? '18:26' : '23:18'}</b>
                  <span>ChatOS 工作站</span>
                </div>
                <div className="monitor-login-preview__user">
                  <i><UserRound size={21} /></i>
                  <b>登录 ChatOS</b>
                  <span><LockKeyhole size={8} /> 点击电脑继续</span>
                </div>
              </div>
          ) : <div
              className={`monitor-desktop is-${timeMode}`}
              onPointerDown={(event) => event.stopPropagation()}
              onClick={(event) => event.stopPropagation()}
            >
              <div className="monitor-desktop__topbar">
                <strong>ChatOS</strong>
                <div>
                  <button type="button" aria-label="锁定电脑" onClick={(event) => { event.stopPropagation(); onLock(); }}><LockKeyhole size={8} /></button>
                  <span>{timeMode === 'day' ? '09:41' : timeMode === 'sunset' ? '18:26' : '23:18'}</span>
                </div>
              </div>
              <div className="monitor-desktop__greeting">
                <b>{greeting}</b>
                <span>书房工作站</span>
              </div>
              <div className="monitor-desktop__apps">
                <button
                  type="button"
                  aria-label="打开 AI 聊天"
                  onClick={(event) => {
                    event.stopPropagation();
                    onChat();
                  }}
                >
                  <i className="is-chat"><Bot size={19} strokeWidth={1.8} /></i>
                  <span>AI 聊天</span>
                </button>
                <button
                  type="button"
                  aria-label="打开终端"
                  onClick={(event) => {
                    event.stopPropagation();
                    onTerminal();
                  }}
                >
                  <i className="is-terminal"><TerminalSquare size={19} strokeWidth={1.8} /></i>
                  <span>终端</span>
                </button>
                <button
                  type="button"
                  aria-label="打开远程连接"
                  onClick={(event) => {
                    event.stopPropagation();
                    onRemote();
                  }}
                >
                  <i className="is-remote"><MonitorUp size={19} strokeWidth={1.8} /></i>
                  <span>远程连接</span>
                </button>
              </div>
              <div className="monitor-desktop__dock">
                <span className="is-active" />
                <span />
                <span />
              </div>
            </div>}
          </div>
        </Html>
      </group>

      <mesh position={[0, 1.24, -0.45]} castShadow>
        <boxGeometry args={[0.12, 0.46, 0.12]} />
        <meshStandardMaterial color="#51677b" metalness={0.25} roughness={0.48} />
      </mesh>
      <mesh position={[0, 1.045, -0.37]} castShadow>
        <boxGeometry args={[0.68, 0.055, 0.32]} />
        <meshStandardMaterial color="#526d83" metalness={0.22} roughness={0.5} />
      </mesh>
      <KeyboardAndMouse />
    </group>
  );
}

function Phone({ onOpen, showPreview, timeMode }: { onOpen: () => void; showPreview: boolean; timeMode: TimeMode }) {
  const [hovered, setHovered] = useState(false);
  const displayTime = timeMode === 'day' ? '09:41' : timeMode === 'sunset' ? '18:26' : '23:18';

  return (
    <group
      position={[1.68, 1.19, 0.12]}
      rotation={[-0.12, -0.28, 0]}
      scale={0.42}
      onClick={(event) => {
        event.stopPropagation();
        onOpen();
      }}
      onPointerEnter={(event) => {
        event.stopPropagation();
        setHovered(true);
        document.body.style.cursor = 'pointer';
      }}
      onPointerLeave={() => {
        setHovered(false);
        document.body.style.cursor = 'default';
      }}
    >
      <mesh position={[0, -0.28, -0.08]} rotation={[0.2, 0, 0]} castShadow>
        <boxGeometry args={[0.58, 0.08, 0.62]} />
        <meshStandardMaterial color="#d7b89c" roughness={0.58} />
      </mesh>
      <mesh position={[0, -0.04, -0.18]} rotation={[-0.55, 0, 0]} castShadow>
        <boxGeometry args={[0.08, 0.72, 0.09]} />
        <meshStandardMaterial color="#c9a78c" roughness={0.6} />
      </mesh>
      <RoundedBox args={[0.66, 1.18, 0.1]} radius={0.1} smoothness={4} position={[0, 0.38, 0]} castShadow>
        <meshStandardMaterial color={hovered ? '#ffdae9' : '#24384c'} metalness={0.2} roughness={0.4} />
      </RoundedBox>
      <mesh position={[0, 0.38, 0.058]}>
        <planeGeometry args={[0.56, 1.02]} />
        <meshStandardMaterial
          color="#233859"
          emissive={hovered ? '#c95d9d' : '#5a75ad'}
          emissiveIntensity={hovered ? 0.95 : 0.48}
          toneMapped={false}
        />
      </mesh>
      <mesh position={[0, 0.32, -0.32]}>
        <boxGeometry args={[0.98, 1.62, 0.08]} />
        <meshBasicMaterial transparent opacity={0} depthWrite={false} colorWrite={false} />
      </mesh>
      {showPreview ? (
        <Html center distanceFactor={1.4} position={[0, 0.4, 0.12]} style={{ pointerEvents: 'auto' }}>
          <button
            type="button"
            className="phone-preview-button"
            aria-label="打开桌面手机"
            onPointerDown={(event) => event.stopPropagation()}
            onClick={(event) => {
              event.stopPropagation();
              onOpen();
            }}
          >
            <div className="phone-preview">
              <b>{displayTime}</b>
              <span>4 tasks</span>
              <small>AI online</small>
            </div>
          </button>
        </Html>
      ) : null}
    </group>
  );
}

function DeskArea({
  timeMode,
  computerFocused,
  computerLocked,
  computerScreenCovered,
  computerContent,
  showPhonePreview,
  showCat,
  onChat,
  onDesktop,
  onTerminal,
  onRemote,
  onLock,
  onPhone,
  onCatPet,
}: {
  timeMode: TimeMode;
  computerFocused: boolean;
  computerLocked: boolean;
  computerScreenCovered: boolean;
  computerContent: ReactNode;
  showPhonePreview: boolean;
  showCat: boolean;
  onChat: () => void;
  onDesktop: () => void;
  onTerminal: () => void;
  onRemote: () => void;
  onLock: () => void;
  onPhone: () => void;
  onCatPet: () => void;
}) {
  return (
    <group>
      <RealModel
        url="/assets/models/polyhaven/WoodenTable_01/WoodenTable_01.gltf"
        scale={[2.5, 1.9, 2.5]}
      />

      <Computer
        onDesktop={onDesktop}
        onChat={onChat}
        onTerminal={onTerminal}
        onRemote={onRemote}
        onLock={onLock}
        focused={computerFocused}
        locked={computerLocked}
        screenCovered={computerScreenCovered}
        content={computerContent}
        timeMode={timeMode}
      />
      <Phone onOpen={onPhone} showPreview={showPhonePreview} timeMode={timeMode} />
      <DeskLamp timeMode={timeMode} />
      {showCat ? <Cat onPet={onCatPet} /> : null}

      <RealModel
        url="/assets/models/polyhaven/office_notepads/office_notepads.gltf"
        position={[1.28, 1.145, 0.62]}
        rotation={[0, -0.18, 0]}
        scale={0.4}
      />
      <RealModel
        url="/assets/models/polyhaven/stationery_supplies/stationery_supplies.gltf"
        position={[2.02, 1.24, -0.54]}
        rotation={[0, -0.28, 0]}
        scale={1.05}
      />

      <RealModel
        url="/assets/models/polyhaven/potted_plant_04/potted_plant_04.gltf"
        position={[1.98, 1.07, -0.58]}
        rotation={[0, -0.45, 0]}
        scale={1.32}
      />
    </group>
  );
}

const PROJECT_BOOKS_PER_PAGE = 6;

function ProjectBookshelf({
  projects,
  focused,
  onFocus,
  onProjectSelect,
}: {
  projects: DemoProject[];
  focused: boolean;
  onFocus: () => void;
  onProjectSelect: (project: DemoProject) => void;
}) {
  const [hovered, setHovered] = useState(false);
  const [page, setPage] = useState(0);
  const pageCount = Math.max(1, Math.ceil(projects.length / PROJECT_BOOKS_PER_PAGE));
  const visibleProjects = projects.slice(page * PROJECT_BOOKS_PER_PAGE, (page + 1) * PROJECT_BOOKS_PER_PAGE);
  useEffect(() => {
    setPage((current) => Math.min(current, pageCount - 1));
  }, [pageCount]);

  return (
    <group
      position={[-5.5, 0.01, -0.55]}
      rotation={[0, Math.PI / 2, 0]}
      onClick={(event) => {
        event.stopPropagation();
        onFocus();
      }}
      onPointerEnter={(event) => {
        event.stopPropagation();
        setHovered(true);
        document.body.style.cursor = 'pointer';
      }}
      onPointerLeave={() => {
        setHovered(false);
        document.body.style.cursor = 'default';
      }}
    >
      <RealModel
        url="/assets/models/polyhaven/wooden_bookshelf_worn/wooden_bookshelf_worn.gltf"
        scale={1.55}
      />
      <mesh position={[0, 1.58, 0.08]}>
        <boxGeometry args={[2.18, 3.2, 0.86]} />
        <meshBasicMaterial transparent opacity={0} depthWrite={false} colorWrite={false} />
      </mesh>

      <RealModel
        url="/assets/models/polyhaven/book_encyclopedia_set_01/book_encyclopedia_set_01.gltf"
        position={[-0.68, 2.45, 0.39]}
        rotation={[0, 0.05, 0]}
        scale={1.45}
      />
      <RealModel
        url="/assets/models/polyhaven/book_encyclopedia_set_01/book_encyclopedia_set_01.gltf"
        position={[-0.75, 0.28, 0.39]}
        rotation={[0, -0.03, 0]}
        scale={1.4}
      />

      {visibleProjects.map((project, index) => {
        const column = index % 3;
        const row = Math.floor(index / 3);
        return (
          <group
            key={project.id}
            position={[-0.52 + column * 0.52, 1.22 + row * 0.76, 0.47 + (column === 1 ? 0.025 : 0)]}
            rotation={[0, 0, column === 0 ? -0.035 : column === 2 ? 0.035 : 0]}
            onClick={(event) => {
              event.stopPropagation();
              onProjectSelect(project);
            }}
          >
            <RoundedBox args={[0.42, 0.76, 0.2]} radius={0.035} smoothness={3} castShadow>
              <meshStandardMaterial
                color={hovered || focused ? project.accent : new Color(project.accent).multiplyScalar(0.78)}
                roughness={0.8}
              />
            </RoundedBox>
            <mesh position={[-0.17, 0, 0.115]} castShadow>
              <boxGeometry args={[0.035, 0.72, 0.012]} />
              <meshStandardMaterial color="#d2b37c" metalness={0.14} roughness={0.56} />
            </mesh>
            <mesh position={[0, 0.2, 0.112]}>
              <boxGeometry args={[0.28, 0.12, 0.016]} />
              <meshStandardMaterial color="#e7dcc5" roughness={0.88} />
            </mesh>
            <Html transform center distanceFactor={1.9} position={[0, -0.08, 0.122]} style={{ pointerEvents: 'auto' }}>
              <button
                type="button"
                className="bookshelf-project-spine"
                onPointerDown={(event) => event.stopPropagation()}
                onClick={(event) => {
                  event.stopPropagation();
                  onProjectSelect(project);
                }}
              >
                <b>{project.name}</b>
                <span>{project.status === 'running' ? '进行中' : project.status === 'planning' ? '规划中' : '已归档'}</span>
              </button>
            </Html>
          </group>
        );
      })}

      <Html center distanceFactor={3.05} position={[0, 0.58, 0.5]} style={{ pointerEvents: 'auto' }}>
        <div
          className={focused ? 'bookshelf-pagination is-focused' : 'bookshelf-pagination'}
          onPointerDown={(event) => event.stopPropagation()}
          onClick={(event) => event.stopPropagation()}
        >
          <button type="button" aria-label="上一页项目" disabled={page === 0} onClick={() => setPage((current) => Math.max(0, current - 1))}>‹</button>
          <span><b>用户项目 · 每页 6 册</b><small>{page + 1} / {pageCount} · 共 {projects.length} 册</small></span>
          <button type="button" aria-label="下一页项目" disabled={page >= pageCount - 1} onClick={() => setPage((current) => Math.min(pageCount - 1, current + 1))}>›</button>
        </div>
      </Html>
    </group>
  );
}

const TASK_COLORS: Record<DemoTask['status'], string> = {
  doing: '#62b4ff',
  todo: '#a9b6c7',
  blocked: '#ff869d',
  done: '#73d9aa',
};

function TaskWallDisplay({
  tasks,
  timeMode,
  onOpen,
  focused,
  content,
}: {
  tasks: DemoTask[];
  timeMode: TimeMode;
  onOpen: () => void;
  focused: boolean;
  content: ReactNode;
}) {
  const [hovered, setHovered] = useState(false);

  return (
    <group
      position={[5.78, 2.32, -0.42]}
      rotation={[0, -Math.PI / 2, 0]}
      onClick={(event) => {
        event.stopPropagation();
        if (!focused) onOpen();
      }}
      onPointerEnter={(event) => {
        event.stopPropagation();
        if (!focused) {
          setHovered(true);
          document.body.style.cursor = 'pointer';
        }
      }}
      onPointerLeave={() => {
        setHovered(false);
        document.body.style.cursor = 'default';
      }}
    >
      <RoundedBox args={[3.98, 2.38, 0.1]} radius={0.055} smoothness={3} position={[0, 0, -0.035]} castShadow>
        <meshStandardMaterial color="#171c20" metalness={0.68} roughness={0.25} />
      </RoundedBox>
      <mesh position={[0, 0, 0.025]}>
        <planeGeometry args={[3.78, 2.18]} />
        <meshPhysicalMaterial
          color="#081927"
          emissive={hovered || focused ? '#4c84b2' : '#254867'}
          emissiveIntensity={focused ? 0.38 : hovered ? 0.3 : 0.16}
          transparent
          opacity={focused ? 0.9 : 0.72}
          roughness={0.16}
          clearcoat={0.9}
          clearcoatRoughness={0.1}
          toneMapped={false}
        />
      </mesh>

      {focused ? (
        <Html center distanceFactor={2.5} position={[0, 0, 0.07]} style={{ pointerEvents: 'auto' }}>
          <div onPointerDown={(event) => event.stopPropagation()} onClick={(event) => event.stopPropagation()}>
            {content}
          </div>
        </Html>
      ) : (
        <Html transform center distanceFactor={5.15} position={[0, 0, 0.07]} style={{ pointerEvents: 'none' }}>
          {tasks.length > 0 ? (
            <div className="task-wall-preview">
              <div className="task-wall-preview__header">
                <b>LIVE TASKS</b>
                <span>{tasks.length} running</span>
              </div>
              {tasks.slice(0, 3).map((task) => (
                <div className="task-wall-preview__task" key={task.id}>
                  <i style={{ background: TASK_COLORS[task.status] }} />
                  <span>{task.title}</span>
                  <em>{task.progress}%</em>
                </div>
              ))}
            </div>
          ) : (
            <div className={`task-wall-preview is-empty is-${timeMode}`}>
              <img src={`/assets/window-${timeMode}.jpg`} alt="" />
              <div className="task-wall-preview__empty-copy">
                <b>WORKSPACE STANDBY</b>
                <span>暂无正在执行的任务</span>
              </div>
            </div>
          )}
        </Html>
      )}

      {[[-1.95, 1.2], [1.95, 1.2], [-1.95, -1.2], [1.95, -1.2]].map(([x, y], index) => (
        <group key={index} position={[x, y, 0.08]}>
          <mesh position={[x < 0 ? 0.11 : -0.11, 0, 0]}>
            <boxGeometry args={[0.22, 0.035, 0.025]} />
            <meshBasicMaterial color="#9ed5ff" transparent opacity={0.7} />
          </mesh>
          <mesh position={[0, y < 0 ? 0.11 : -0.11, 0]}>
            <boxGeometry args={[0.035, 0.22, 0.025]} />
            <meshBasicMaterial color="#9ed5ff" transparent opacity={0.7} />
          </mesh>
        </group>
      ))}
    </group>
  );
}

function AmbientDecor({ timeMode }: { timeMode: TimeMode }) {
  return (
    <>
      <RealModel
        url="/assets/models/polyhaven/potted_plant_04/potted_plant_04.gltf"
        position={[-2.72, 0.02, -2.85]}
        rotation={[0, 0.45, 0]}
        scale={0.88}
      />

      <group position={[0, 4.32, 0.85]}>
        <mesh castShadow>
          <cylinderGeometry args={[0.045, 0.045, 1.0, 16]} />
          <meshStandardMaterial color="#4b4b4a" metalness={0.65} roughness={0.28} />
        </mesh>
        <mesh position={[0, -0.56, 0]} castShadow>
          <cylinderGeometry args={[0.58, 0.26, 0.34, 32, 1, true]} />
          <meshStandardMaterial color="#3e4144" metalness={0.35} roughness={0.38} side={2} />
        </mesh>
        <pointLight
          position={[0, -0.78, 0]}
          intensity={timeMode === 'night' ? 1.6 : 0.28}
          color="#ffd7a6"
          distance={8}
        />
      </group>
    </>
  );
}

export function RealisticRoom({
  view,
  timeMode,
  projects,
  tasks,
  computerLocked,
  computerScreenCovered,
  computerContent,
  taskWallContent,
  onViewChange,
  onComputerLock,
  onProjectSelect,
  onCatPet,
}: RealisticRoomProps) {
  const { scene, invalidate } = useThree();

  useEffect(() => {
    invalidate();
  }, [invalidate, timeMode]);

  useFrame(() => {
    const targetColor = new Color(
      timeMode === 'night' ? '#101b34' : timeMode === 'sunset' ? '#9b7ea5' : '#82bfe6',
    );
    if (scene.background instanceof Color) {
      scene.background.lerp(targetColor, 0.085);
      const colorDelta = Math.abs(scene.background.r - targetColor.r)
        + Math.abs(scene.background.g - targetColor.g)
        + Math.abs(scene.background.b - targetColor.b);
      if (colorDelta > 0.002) invalidate();
    } else {
      scene.background = targetColor;
    }
  });

  return (
    <>
      <CameraRig view={view} />
      <RendererExposure timeMode={timeMode} />
      <Environment
        files="/assets/environment/lebombo_1k.hdr"
        background={false}
        environmentIntensity={timeMode === 'night' ? 0.16 : timeMode === 'sunset' ? 0.28 : 0.38}
      />
      <SceneLighting timeMode={timeMode} />
      <RoomShell timeMode={timeMode} />
      <WindowView timeMode={timeMode} />
      <Curtains timeMode={timeMode} />
      <DeskArea
        timeMode={timeMode}
        computerFocused={!computerScreenCovered && (view === 'chat' || view === 'terminal' || view === 'remote' || (view === 'computer' && computerLocked))}
        computerLocked={computerLocked}
        computerScreenCovered={computerScreenCovered}
        computerContent={computerContent}
        showPhonePreview={view === 'room'}
        showCat={view === 'room' || view === 'computer' || view === 'chat' || view === 'terminal' || view === 'remote' || view === 'phone'}
        onDesktop={() => onViewChange('computer')}
        onChat={() => onViewChange('chat')}
        onTerminal={() => onViewChange('terminal')}
        onRemote={() => onViewChange('remote')}
        onLock={onComputerLock}
        onPhone={() => onViewChange('phone')}
        onCatPet={onCatPet}
      />
      {view !== 'project' ? (
        <ProjectBookshelf
          projects={projects}
          focused={view === 'archive'}
          onFocus={() => onViewChange('archive')}
          onProjectSelect={onProjectSelect}
        />
      ) : null}
      <TaskWallDisplay
        tasks={tasks}
        timeMode={timeMode}
        onOpen={() => onViewChange('projection')}
        focused={view === 'projection'}
        content={taskWallContent}
      />
      <AmbientDecor timeMode={timeMode} />

      <ContactShadows
        position={[0, 0.026, 0]}
        opacity={timeMode === 'night' ? 0.42 : 0.28}
        scale={12}
        blur={2.4}
        far={7}
        frames={1}
      />
    </>
  );
}
