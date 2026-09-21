import { invoke } from "@tauri-apps/api/core";
import { useAppStore } from "../store";
import type { DnsRecord } from "../types";

export function useDns() {
  const setError = useAppStore((s) => s.setError);

  const getRecords = async (): Promise<DnsRecord[]> => {
    try {
      return await invoke<DnsRecord[]>("get_dns_records");
    } catch (e) {
      setError(String(e));
      return [];
    }
  };

  const addRecord = async (record: DnsRecord): Promise<boolean> => {
    try {
      await invoke("add_dns_record", { record });
      return true;
    } catch (e) {
      setError(String(e));
      return false;
    }
  };

  const removeRecord = async (domain: string, recordType: string): Promise<boolean> => {
    try {
      await invoke("remove_dns_record", { domain, recordType });
      return true;
    } catch (e) {
      setError(String(e));
      return false;
    }
  };

  return { getRecords, addRecord, removeRecord };
}
