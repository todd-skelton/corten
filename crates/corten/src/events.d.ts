// The typed events the script tests' fixtures import (an app's come from its generated
// declarations, see corten::types).
declare module "corten/events" {
  export class RowToggled extends Event {
    constructor(init: { id: string; selected: boolean });
    readonly id: string;
    readonly selected: boolean;
  }
}

export {};
