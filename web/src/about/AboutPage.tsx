import { Text } from "@astryxdesign/core/Text";
import * as stylex from "@stylexjs/stylex";
import { Page, PageHeader } from "#/ui/Page.tsx";
import { Sheet, SheetBody } from "#/ui/Sheet.tsx";

/**
 * About: what AWBRN is and how it is built, on one sheet that fills the
 * column. Independence is said once, in the footer every page ends with; no
 * keys, since the top bar already goes everywhere this page could send.
 */
export function AboutPage() {
  return (
    <Page width="narrow">
      <PageHeader title="About" />

      <Sheet>
        <SheetBody xstyle={styles.body}>
          <Text>
            AWBRN, said auburn, is a browser client for Advance Wars. Join a lobby, start a match on
            any map in the catalog, or let ranked find an opponent. Turns arrive live, and every
            finished match can be read back turn by turn. Replays from AWBW load from a file that
            stays on your device.
          </Text>
          <Text>
            The game runs in a Rust engine compiled to WebAssembly and draws the game&apos;s own
            sprites. Its rules come from AWVM, an executable specification of how Advance Wars
            plays.
          </Text>
        </SheetBody>
      </Sheet>
    </Page>
  );
}

const styles = stylex.create({
  body: {
    gap: "12px",
    paddingBlockStart: "20px",
  },
});
