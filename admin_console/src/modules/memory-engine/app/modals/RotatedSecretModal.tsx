// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { Button, Card, Modal, Typography } from 'antd';

import type { RotateSourceSecretResponse } from '../../types';

const { Paragraph, Text } = Typography;

type RotatedSecretModalProps = {
  rotatedSecret: RotateSourceSecretResponse | null;
  onClose: () => void;
};

export function RotatedSecretModal(props: RotatedSecretModalProps) {
  const { rotatedSecret, onClose } = props;

  return (
    <Modal
      open={Boolean(rotatedSecret)}
      title="新的接入密钥"
      footer={[
        <Button key="ok" type="primary" onClick={onClose}>
          我知道了
        </Button>,
      ]}
      onCancel={onClose}
      destroyOnClose
    >
      <Paragraph>该密钥只会在这里展示一次，请立即同步到接入系统配置中。</Paragraph>
      <Card size="small">
        <Text code copyable={{ text: rotatedSecret?.secret_key ?? '' }}>
          {rotatedSecret?.secret_key ?? ''}
        </Text>
      </Card>
      {rotatedSecret ? (
        <Paragraph type="secondary" style={{ marginTop: 12 }}>
          系统：{rotatedSecret.source.name} ({rotatedSecret.source.source_id})
        </Paragraph>
      ) : null}
    </Modal>
  );
}
