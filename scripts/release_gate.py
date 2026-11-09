#!/usr/bin/env python3
"""Fail closed: automated checks never stand in for actual traffic/signing evidence."""
import json,pathlib,sys
root=pathlib.Path(__file__).resolve().parents[1]
evidence=root/'docs/release-evidence.json'
required=['windows_clean_install','windows_real_voice_video_quic','windows_crash_reboot_uninstall',
          'macos_arm64_packet_path','macos_arm64_real_voice_video_quic','macos_intel_packet_path',
          'macos_intel_real_voice_video_quic','macos_guard_recovery','apple_signed_notarized',
          'windows_authenticode','signed_updates_rollback','third_party_notices_sources','security_review']
data=json.loads(evidence.read_text()) if evidence.exists() else {}
missing=[name for name in required if not isinstance(data.get(name),dict) or data[name].get('passed') is not True or not data[name].get('evidence')]
if missing:
 print('Production release is BLOCKED. Missing verified evidence:')
 for name in missing:print(' - '+name)
 sys.exit(1)
print('Recorded production release gates are complete. Review the evidence before publishing.')
