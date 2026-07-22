import { ContactShadows, Environment } from '@react-three/drei';
import { useFrame, useThree } from '@react-three/fiber';
import { type ReactNode, useEffect } from 'react';
import { Color } from 'three';
import type { DemoProject, DemoTask, TimeMode, ViewMode } from '../types';
import { WindowView } from './WindowView';
import { RealModel } from './room/RealModel';
import { CameraRig, Curtains, RendererExposure, RoomShell, SceneLighting } from './room/architecture';
import { DeskArea } from './room/desk';
import { ProjectBookshelf, TaskWallDisplay } from './room/walls';

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
