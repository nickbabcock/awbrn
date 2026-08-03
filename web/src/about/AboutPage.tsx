import { Card } from "@astryxdesign/core/Card";
import { Code } from "@astryxdesign/core/Code";
import { Grid } from "@astryxdesign/core/Grid";
import { Section } from "@astryxdesign/core/Section";
import { HStack, VStack } from "@astryxdesign/core/Stack";
import { Heading, Text } from "@astryxdesign/core/Text";
import { Token } from "@astryxdesign/core/Token";
import { TWO_COLUMN_GRID_MIN_WIDTH } from "#/ui/layout.ts";

const acronym = [
  { letter: "A", word: "Advance", color: "red" },
  { letter: "W", word: "Wars", color: "blue" },
  { letter: "B", word: "By", color: "green" },
  { letter: "R", word: "Rust", color: "yellow" },
  { letter: "N", word: "(New)", color: "purple" },
] as const;

export function AboutPage() {
  return (
    <Section padding={6} variant="transparent">
      <VStack gap={6} width="100%">
        <Heading level={1}>What&apos;s in a name</Heading>

        <Grid
          align="start"
          columns={{ minWidth: TWO_COLUMN_GRID_MIN_WIDTH, max: 2, repeat: "fit" }}
          gap={6}
        >
          <Card elevation="high" padding={6}>
            <VStack gap={4}>
              <Heading level={2}>Built for battle review</Heading>
              <Text type="large" weight="normal">
                AWBRN, pronounced auburn, is a replay viewer and game toolkit for Advance Wars By
                Web. It is built to make battle review readable at a glance, with recognizable CO
                portraits, stable terrain rendering, and a browser-native flow backed by Rust and
                WebAssembly.
              </Text>
            </VStack>
          </Card>

          <Card elevation="high" padding={6} variant="muted">
            <VStack gap={4}>
              <Heading level={2}>AWBRN</Heading>
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
              <Text color="secondary">
                Load a <Code>.zip</Code> replay, step through every turn, and inspect the
                battlefield without losing the character of the source game.
              </Text>
            </VStack>
          </Card>
        </Grid>
      </VStack>
    </Section>
  );
}
