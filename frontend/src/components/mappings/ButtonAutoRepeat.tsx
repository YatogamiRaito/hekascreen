import { useEffect, useMemo, useState } from "react";
import type { MappingUpdater, AutoRepeatConfig } from "./mapping";
import { Flex, Input, InputNumber, Tooltip, Typography } from "antd";
import {
  mappingButtonDragFactory,
  mappingButtonPresetStyle,
  mappingButtonTransformStyle,
} from "./tools";
import { useAppSelector } from "../../store/store";
import { ItemBoxContainer, ItemBox } from "../common/ItemBox";
import {
  SettingBind,
  SettingFooter,
  SettingModal,
  SettingNote,
} from "./Common";
import { useTranslation } from "react-i18next";

const PRESET_STYLE = mappingButtonPresetStyle(52);

export default function ButtonAutoRepeat({
  index,
  config,
  originalSize,
  onConfigChange,
  onConfigDelete,
  onConfigCopy,
}: {
  index: number;
  config: AutoRepeatConfig;
  originalSize: { width: number; height: number };
  onConfigChange: MappingUpdater<AutoRepeatConfig>;
  onConfigDelete: () => void;
  onConfigCopy: () => void;
}) {
  const id = `mapping-auto-repeat-${index}`;
  const bindText = config.bind.length > 0 ? config.bind.join("+") : "???";
  const className =
    "rounded-full absolute box-border border-solid border-2 color-text " +
    (config.bind.length > 0
      ? "border-text-secondary hover:border-text"
      : "border-primary hover:border-primary-hover");

  const maskArea = useAppSelector((state) => state.other.maskArea);
  const [showSetting, setShowSetting] = useState(false);

  const scale = useMemo(() => {
    return {
      x: maskArea.width / originalSize.width,
      y: maskArea.height / originalSize.height,
    };
  }, [originalSize, maskArea]);

  useEffect(() => {
    const element = document.getElementById(id);
    if (element) {
      element.style.transform = mappingButtonTransformStyle(
        config.position.x,
        config.position.y,
        scale
      );
    }
  }, [index, config, scale]);

  const handleDrag = mappingButtonDragFactory(
    maskArea,
    originalSize,
    ({ x, y }) => {
      onConfigChange({
        ...config,
        position: {
          x,
          y,
        },
      });
    }
  );

  const handleSetting = (e: React.MouseEvent) => {
    e.preventDefault();
    setShowSetting(true);
  };

  return (
    <>
      <SettingModal open={showSetting} onClose={() => setShowSetting(false)}>
        <Setting
          config={config}
          onConfigChange={onConfigChange}
          onConfigDelete={() => {
            setShowSetting(false);
            onConfigDelete();
          }}
          onConfigCopy={() => {
            setShowSetting(false);
            onConfigCopy();
          }}
        />
      </SettingModal>
      <Flex
        id={id}
        style={PRESET_STYLE}
        className={className}
        onMouseDown={handleDrag}
        onContextMenu={handleSetting}
        justify="center"
        align="center"
        vertical
      >
        <Tooltip trigger="click" title={`${config.type}: ${bindText} -> ${config.target_key}`}>
          <Typography.Text ellipsis={true} className="text-2.5 font-bold">
            {bindText}
          </Typography.Text>
        </Tooltip>
        <Typography.Text ellipsis={true} className="text-2 font-mono color-primary">
          {config.target_key}
        </Typography.Text>
      </Flex>
    </>
  );
}

function Setting({
  config,
  onConfigChange,
  onConfigDelete,
  onConfigCopy,
}: {
  config: AutoRepeatConfig;
  onConfigChange: MappingUpdater<AutoRepeatConfig>;
  onConfigDelete: () => void;
  onConfigCopy: () => void;
}) {
  const { t } = useTranslation();

  return (
    <div>
      <h1 className="title-with-line">
        {t("mappings.autoRepeat.setting.title")}
      </h1>
      <ItemBoxContainer className="max-h-70vh overflow-y-auto pr-2 scrollbar">
        <SettingBind
          bind={config.bind}
          onBindChange={(bind) => onConfigChange((pre) => ({ ...pre, bind }))}
        />
        <ItemBox label={t("mappings.autoRepeat.setting.target_key")}>
          <Input
            className="w-full"
            value={config.target_key}
            onChange={(e) =>
              onConfigChange({ ...config, target_key: e.target.value })
            }
          />
        </ItemBox>
        <ItemBox label={t("mappings.autoRepeat.setting.interval")}>
          <InputNumber
            className="w-full"
            value={config.interval}
            min={1}
            suffix="ms"
            onChange={(v) =>
              v !== null && onConfigChange({ ...config, interval: v })
            }
          />
        </ItemBox>
        <SettingNote
          note={config.note}
          onNoteChange={(note) => onConfigChange({ ...config, note })}
        />
        <SettingFooter onDelete={onConfigDelete} onCopy={onConfigCopy} />
      </ItemBoxContainer>
    </div>
  );
}
