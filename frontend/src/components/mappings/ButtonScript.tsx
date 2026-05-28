import { useEffect, useMemo, useState } from "react";
import type { MappingUpdater, ScriptConfig } from "./mapping";
import { Flex, Input, InputNumber, Modal, Tooltip, Typography, Mentions } from "antd";
import {
  mappingButtonDragFactory,
  mappingButtonPresetStyle,
  mappingButtonTransformStyle,
} from "./tools";
import { useAppSelector } from "../../store/store";
import { ItemBox, ItemBoxContainer } from "../common/ItemBox";
import {
  SettingBind,
  SettingFooter,
  SettingModal,
  SettingNote,
} from "./Common";
import { useTranslation } from "react-i18next";
import IconButton from "../common/IconButton";
import { PlayCircleOutlined } from "@ant-design/icons";
import { useDispatch } from "react-redux";
import { setIsLoading } from "../../store/other";
import { requestPost } from "../../utils";
import { IconFont, useMessageContext } from "../../hooks";

const PRESET_STYLE = mappingButtonPresetStyle(52);

const SCRIPT_AUTOCOMPLETE_OPTIONS = [
  { value: "tap(0, 500, 500, \"default\")", label: "tap(pointer_id, x, y, action)" },
  { value: "swipe(0, 20, 100, 100, 500, 500)", label: "swipe(pointer_id, interval, x1, y1, x2, y2)" },
  { value: "sleep(100)", label: "sleep(ms)" },
  { value: "wait(100)", label: "wait(ms)" },
  { value: "toggle(1)", label: "toggle(id)" },
  { value: "set_toggle(1, true)", label: "set_toggle(id, value)" },
  { value: "get_toggle(1)", label: "get_toggle(id)" },
  { value: "set_var(\"name\", 1)", label: "set_var(name, value)" },
  { value: "get_var(\"name\")", label: "get_var(name)" },
  { value: "has_var(\"name\")", label: "has_var(name)" },
  { value: "del_var(\"name\")", label: "del_var(name)" },
  { value: "send_key(\"KEYCODE_BACK\")", label: "send_key(key_name)" },
  { value: "paste_text(\"text\")", label: "paste_text(text)" },
  { value: "print(\"hello\")", label: "print(...)" },
  { value: "ORIGINAL_W", label: "ORIGINAL_W" },
  { value: "ORIGINAL_H", label: "ORIGINAL_H" },
  { value: "CURSOR_X", label: "CURSOR_X" },
  { value: "CURSOR_Y", label: "CURSOR_Y" },
];

export default function ButtonScript({
  index,
  config,
  originalSize,
  onConfigChange,
  onConfigDelete,
  onConfigCopy,
}: {
  index: number;
  config: ScriptConfig;
  originalSize: { width: number; height: number };
  onConfigChange: MappingUpdater<ScriptConfig>;
  onConfigDelete: () => void;
  onConfigCopy: () => void;
}) {
  const id = `mapping-single-tap-${index}`;
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
        scale,
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
    },
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
        <Tooltip trigger="click" title={`${config.type}: ${bindText}`}>
          <Typography.Text ellipsis={true} className="text-2.5 font-bold">
            {bindText}
          </Typography.Text>
        </Tooltip>
        <IconFont type="icon-code" className="text-4"/>
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
  config: ScriptConfig;
  onConfigChange: MappingUpdater<ScriptConfig>;
  onConfigDelete: () => void;
  onConfigCopy: () => void;
}) {
  const { t } = useTranslation();
  const dispatch = useDispatch();
  const messageApi = useMessageContext();

  const [errorMsg, setErrorMsg] = useState("");
  const [open, setOpen] = useState(false);

  async function run_script(script: string) {
    dispatch(setIsLoading(true));
    try {
      const res = await requestPost("/api/device/control/eval_script", {
        script,
      });
      messageApi?.success(res.message);
    } catch (error: any) {
      setErrorMsg(error);
      setOpen(true);
    }
    dispatch(setIsLoading(false));
  }

  return (
    <div>
      <Modal
        title={t("mappings.script.setting.result")}
        className="min-w-50vw"
        open={open}
        onCancel={() => setOpen(false)}
        footer={null}
      >
        <Input.TextArea
          className="font-mono"
          value={errorMsg}
          readOnly
          autoSize
        />
      </Modal>
      <h1 className="title-with-line">{t("mappings.script.setting.title")}</h1>
      <ItemBoxContainer className="max-h-70vh overflow-y-auto pr-2 scrollbar">
        <SettingBind
          bind={config.bind}
          onBindChange={(bind) => onConfigChange((pre) => ({ ...pre, bind }))}
        />
        <ItemBox label={t("mappings.script.setting.interval")}>
          <InputNumber
            className="w-full"
            value={config.interval}
            min={0}
            suffix="ms"
            onChange={(v) =>
              v !== null && onConfigChange({ ...config, interval: v })
            }
          />
        </ItemBox>
        <ItemBox
          label={
            <Flex className="w-full" align="center" justify="space-between">
              <span>{t("mappings.script.setting.pressed_script")}</span>
              <IconButton
                tooltip={t("mappings.script.setting.run_script")}
                icon={<PlayCircleOutlined />}
                onClick={() => run_script(config.pressed_script)}
              />
            </Flex>
          }
        >
          <Mentions
            className="w-full font-mono"
            value={config.pressed_script}
            placeholder={t(
              "mappings.script.setting.pressed_script_placeholder",
            )}
            autoSize={{ minRows: 2, maxRows: 10 }}
            prefix=""
            options={SCRIPT_AUTOCOMPLETE_OPTIONS}
            onChange={(v) =>
              onConfigChange({ ...config, pressed_script: v })
            }
          />
        </ItemBox>
        <ItemBox
          label={
            <Flex className="w-full" align="center" justify="space-between">
              <span>{t("mappings.script.setting.held_script")}</span>
              <IconButton
                tooltip={t("mappings.script.setting.run_script")}
                icon={<PlayCircleOutlined />}
                onClick={() => run_script(config.held_script)}
              />
            </Flex>
          }
        >
          <Mentions
            className="w-full font-mono"
            value={config.held_script}
            placeholder={t("mappings.script.setting.held_script_placeholder")}
            autoSize={{ minRows: 2, maxRows: 10 }}
            prefix=""
            options={SCRIPT_AUTOCOMPLETE_OPTIONS}
            onChange={(v) =>
              onConfigChange({ ...config, held_script: v })
            }
          />
        </ItemBox>
        <ItemBox
          label={
            <Flex className="w-full" align="center" justify="space-between">
              <span>{t("mappings.script.setting.released_script")}</span>
              <IconButton
                tooltip={t("mappings.script.setting.run_script")}
                icon={<PlayCircleOutlined />}
                onClick={() => run_script(config.released_script)}
              />
            </Flex>
          }
        >
          <Mentions
            className="w-full font-mono"
            value={config.released_script}
            placeholder={t(
              "mappings.script.setting.released_script_placeholder",
            )}
            autoSize={{ minRows: 2, maxRows: 10 }}
            prefix=""
            options={SCRIPT_AUTOCOMPLETE_OPTIONS}
            onChange={(v) =>
              onConfigChange({ ...config, released_script: v })
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
