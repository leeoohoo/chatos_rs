import { Html, RoundedBox } from '@react-three/drei';
import { Bot, LockKeyhole, MonitorUp, TerminalSquare, UserRound } from 'lucide-react';
import { type ReactNode, useState } from 'react';
import type { TimeMode } from '../../types';
import { Cat } from '../Cat';
import { RealModel } from './RealModel';

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

export function DeskArea({
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
