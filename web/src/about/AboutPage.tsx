import { Card } from "@astryxdesign/core/Card";
import { Heading } from "@astryxdesign/core/Heading";
import { HStack, VStack } from "@astryxdesign/core/Stack";
import { Text } from "@astryxdesign/core/Text";
import { Token } from "@astryxdesign/core/Token";
import { RouterButton } from "#/ui/astryx-links.tsx";
import { Page, PageHeader } from "#/ui/Page.tsx";

const acronym = [
  { letter: "A", word: "Advance", color: "red" },
  { letter: "W", word: "Wars", color: "blue" },
  { letter: "B", word: "By", color: "green" },
  { letter: "R", word: "Rust", color: "yellow" },
  { letter: "N", word: "(New)", color: "purple" },
] as const;

export function AboutPage() {
  return (
    <Page width="narrow">
      <PageHeader
        description="AWBRN, pronounced auburn, is a browser client for Advance Wars By Web players."
        title="About AWBRN"
      />

      <VStack as="section" gap={3}>
        <Heading level={2}>What it is for</Heading>
        <Text>
          AWBRN is a place to play Advance Wars matches without leaving the browser. Join an open
          lobby, start a match on a map from the catalog or one brought over from AWBW, or let the
          ranked queue find opponents near your rating. Turns arrive live, and a match keeps its
          address after it ends, so the finished battle can be read back turn by turn.
        </Text>
        <Text>
          It also reads replays. Load an AWBW replay archive and step through every turn, with each
          army&apos;s CO, funds, and unit strength beside the board. The file stays on your device.
        </Text>
      </VStack>

      <VStack as="section" gap={3}>
        <Heading level={2}>How it is built</Heading>
        <Text>
          The game runs in a Rust engine compiled to WebAssembly, and the board is drawn from the
          game&apos;s own sprites at their native pixels. The rules come from AWVM, an executable
          specification of how Advance Wars plays, so what the screen says about a cost or an
          outcome is what the engine does.
        </Text>
      </VStack>

      <Card padding={6} variant="muted">
        <VStack gap={4}>
          <Heading level={2}>The name</Heading>
          <VStack as="ul" gap={2} role="list">
            {acronym.map(({ letter, word, color }) => (
              <HStack as="li" key={letter} align="center" gap={3}>
                <Token color={color} label={letter} size="lg" />
                <Text as="span" type="large" weight="bold">
                  {word}
                </Text>
              </HStack>
            ))}
          </VStack>
        </VStack>
      </Card>

      <VStack as="section" gap={3}>
        <Heading level={2}>Independent</Heading>
        <Text>
          AWBRN is not affiliated with, endorsed by, or connected to Advance Wars By Web, Nintendo,
          or Intelligent Systems.
        </Text>
        <HStack gap={2} wrap="wrap">
          <RouterButton label="Find a match" to="/matches" variant="primary" />
          <RouterButton label="Review a replay" to="/replay" variant="secondary" />
        </HStack>
      </VStack>
    </Page>
  );
}
