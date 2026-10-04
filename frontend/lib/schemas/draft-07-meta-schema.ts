import type { JSONSchema7 } from "json-schema"

// Official draft-07 meta-schema, enriched with descriptions so editor
// hovers/autocomplete show useful docs. Descriptions are annotations only
// and do not affect validation.
export const DRAFT_07_META_SCHEMA: JSONSchema7 = {
  $schema: "http://json-schema.org/draft-07/schema#",
  $id: "http://json-schema.org/draft-07/schema#",
  title: "Core schema meta-schema",
  definitions: {
    schemaArray: {
      type: "array",
      minItems: 1,
      items: { $ref: "#" },
    },
    nonNegativeInteger: {
      type: "integer",
      minimum: 0,
    },
    nonNegativeIntegerDefault0: {
      allOf: [{ $ref: "#/definitions/nonNegativeInteger" }, { default: 0 }],
    },
    simpleTypes: {
      enum: ["array", "boolean", "integer", "null", "number", "object", "string"],
    },
    stringArray: {
      type: "array",
      items: { type: "string" },
      uniqueItems: true,
      default: [],
    },
  },
  type: ["object", "boolean"],
  properties: {
    $id: {
      type: "string",
      format: "uri-reference",
      description: "A canonical URI for this schema.",
    },
    $schema: {
      type: "string",
      format: "uri",
      description: "The JSON Schema dialect this document conforms to.",
    },
    $ref: {
      type: "string",
      format: "uri-reference",
      description: "Reference another schema by URI pointer.",
    },
    $comment: {
      type: "string",
      description: "A comment for schema maintainers.",
    },
    title: {
      type: "string",
      description: "A short explanation of the schema's purpose.",
    },
    description: {
      type: "string",
      description: "A longer explanation of the schema's purpose.",
    },
    default: true,
    readOnly: {
      type: "boolean",
      default: false,
      description: "If true, the value is managed by the owner, not clients.",
    },
    writeOnly: {
      type: "boolean",
      default: false,
      description: "If true, the value is write-only (e.g. a password).",
    },
    examples: {
      type: "array",
      items: true,
      description: "Example values satisfying this schema.",
    },
    multipleOf: {
      type: "number",
      exclusiveMinimum: 0,
      description: "Value must be a multiple of this number.",
    },
    maximum: {
      type: "number",
      description: "Inclusive upper bound for numbers.",
    },
    exclusiveMaximum: {
      type: "number",
      description: "Exclusive upper bound for numbers.",
    },
    minimum: {
      type: "number",
      description: "Inclusive lower bound for numbers.",
    },
    exclusiveMinimum: {
      type: "number",
      description: "Exclusive lower bound for numbers.",
    },
    maxLength: { $ref: "#/definitions/nonNegativeInteger", description: "Maximum string length." },
    minLength: {
      $ref: "#/definitions/nonNegativeIntegerDefault0",
      description: "Minimum string length.",
    },
    pattern: {
      type: "string",
      format: "regex",
      description: "ECMA-262 regex the string must match.",
    },
    additionalItems: { $ref: "#", description: "Schema for array items beyond those in items." },
    items: {
      anyOf: [{ $ref: "#" }, { $ref: "#/definitions/schemaArray" }],
      default: true,
      description: "Schema(s) applied to array items.",
    },
    maxItems: { $ref: "#/definitions/nonNegativeInteger", description: "Maximum array length." },
    minItems: {
      $ref: "#/definitions/nonNegativeIntegerDefault0",
      description: "Minimum array length.",
    },
    uniqueItems: {
      type: "boolean",
      default: false,
      description: "If true, all array items must be unique.",
    },
    contains: { $ref: "#", description: "Schema that at least one array item must satisfy." },
    maxProperties: {
      $ref: "#/definitions/nonNegativeInteger",
      description: "Maximum number of object properties.",
    },
    minProperties: {
      $ref: "#/definitions/nonNegativeIntegerDefault0",
      description: "Minimum number of object properties.",
    },
    required: {
      $ref: "#/definitions/stringArray",
      description: "Property names that must be present.",
    },
    additionalProperties: {
      $ref: "#",
      description: "Schema for properties not listed in properties/patternProperties.",
    },
    definitions: {
      type: "object",
      additionalProperties: { $ref: "#" },
      default: {},
      description: "Reusable schema definitions, referenced via $ref.",
    },
    properties: {
      type: "object",
      additionalProperties: { $ref: "#" },
      default: {},
      description: "Schema per allowed property name.",
    },
    patternProperties: {
      type: "object",
      additionalProperties: { $ref: "#" },
      propertyNames: { format: "regex" },
      default: {},
      description: "Schema for properties whose names match a regex.",
    },
    dependencies: {
      type: "object",
      additionalProperties: {
        anyOf: [{ $ref: "#" }, { $ref: "#/definitions/stringArray" }],
      },
      description: "Conditional requirements based on other properties.",
    },
    propertyNames: { $ref: "#", description: "Schema that every property name must satisfy." },
    const: true,
    enum: {
      type: "array",
      items: true,
      minItems: 1,
      uniqueItems: true,
      description: "The value must be one of these exact values.",
    },
    type: {
      anyOf: [
        { $ref: "#/definitions/simpleTypes" },
        {
          type: "array",
          items: { $ref: "#/definitions/simpleTypes" },
          minItems: 1,
          uniqueItems: true,
        },
      ],
      description: "Allowed JSON type(s): array, boolean, integer, null, number, object, string.",
    },
    format: {
      type: "string",
      description: "Well-known string format, e.g. date-time, uri, email, uuid.",
    },
    contentMediaType: {
      type: "string",
      description: "MIME type of the string's content, e.g. image/png.",
    },
    contentEncoding: {
      type: "string",
      description: "Encoding of the string's content, e.g. base64.",
    },
    if: { $ref: "#", description: "Schema that selects then/else branches." },
    then: { $ref: "#", description: "Applied when the if schema validates." },
    else: { $ref: "#", description: "Applied when the if schema fails." },
    allOf: { $ref: "#/definitions/schemaArray", description: "Value must validate against all of these." },
    anyOf: { $ref: "#/definitions/schemaArray", description: "Value must validate against at least one of these." },
    oneOf: {
      $ref: "#/definitions/schemaArray",
      description: "Value must validate against exactly one of these.",
    },
    not: { $ref: "#", description: "Value must NOT validate against this schema." },
  },
  default: true,
}
