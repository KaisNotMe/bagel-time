// Which account will be used to play, shared by the sidebar and settings.
import { api, type AccountList } from "./api";

class AccountState {
  list = $state<AccountList>({ accounts: [], active: null });

  get activeName(): string | null {
    return this.list.accounts.find((a) => a.uuid === this.list.active)?.username ?? null;
  }

  async refresh() {
    try {
      this.list = await api.listAccounts();
    } catch {
      // Leave the last known list; the settings page shows errors.
    }
  }
}

export const account = new AccountState();
