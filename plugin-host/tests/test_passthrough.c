// Minimal CLAP passthrough plugin for host testing.
// Compiles with: cc -shared -fPIC test_passthrough.c -o test_passthrough.so

#include <stdbool.h>
#include <stdint.h>
#include <string.h>

#define CLAP_VERSION_MAJOR 1
#define CLAP_VERSION_MINOR 1
#define CLAP_VERSION_REVISION 10
#define CLAP_VERSION_INIT {1, 1, 10}

typedef struct clap_version {
    uint32_t major;
    uint32_t minor;
    uint32_t revision;
} clap_version_t;

typedef struct clap_host {
    clap_version_t clap_version;
    void *host_data;
    const char *name;
    const char *vendor;
    const char *url;
    const char *version;
    const void *(*get_extension)(const struct clap_host *, const char *);
    void (*request_restart)(const struct clap_host *);
    void (*request_process)(const struct clap_host *);
    void (*request_callback)(const struct clap_host *);
} clap_host_t;

typedef struct clap_plugin_descriptor {
    clap_version_t clap_version;
    const char *id;
    const char *name;
    const char *vendor;
    const char *url;
    const char *manual_url;
    const char *support_url;
    const char *version;
    const char *description;
    const char **features;
} clap_plugin_descriptor_t;

typedef struct clap_audio_buffer {
    float **data32;
    double **data64;
    uint32_t channel_count;
    uint32_t latency;
    uint64_t constant_mask;
} clap_audio_buffer_t;

typedef struct clap_process {
    int64_t steady_time;
    uint32_t frames_count;
    const void *transport;
    clap_audio_buffer_t *audio_inputs;
    clap_audio_buffer_t *audio_outputs;
    uint32_t audio_inputs_count;
    uint32_t audio_outputs_count;
    const void *in_events;
    const void *out_events;
} clap_process_t;

typedef struct clap_plugin {
    const clap_plugin_descriptor_t *desc;
    void *plugin_data;
    bool (*init)(const struct clap_plugin *);
    void (*destroy)(const struct clap_plugin *);
    bool (*activate)(const struct clap_plugin *, double, uint32_t, uint32_t);
    void (*deactivate)(const struct clap_plugin *);
    bool (*start_processing)(const struct clap_plugin *);
    void (*stop_processing)(const struct clap_plugin *);
    void (*reset)(const struct clap_plugin *);
    int32_t (*process)(const struct clap_plugin *, const clap_process_t *);
    const void *(*get_extension)(const struct clap_plugin *, const char *);
    void (*on_main_thread)(const struct clap_plugin *);
} clap_plugin_t;

typedef struct clap_plugin_factory {
    uint32_t (*get_plugin_count)(const struct clap_plugin_factory *);
    const clap_plugin_descriptor_t *(*get_plugin_descriptor)(const struct clap_plugin_factory *, uint32_t);
    const clap_plugin_t *(*create_plugin)(const struct clap_plugin_factory *, const clap_host_t *, const char *);
} clap_plugin_factory_t;

typedef struct clap_plugin_entry {
    clap_version_t clap_version;
    bool (*init)(const char *);
    void (*deinit)(void);
    const void *(*get_factory)(const char *);
} clap_plugin_entry_t;

/* === Plugin instance === */

static bool plugin_init(const clap_plugin_t *plugin) {
    (void)plugin;
    return true;
}

static void plugin_destroy(const clap_plugin_t *plugin) {
    (void)plugin;
}

static bool plugin_activate(const clap_plugin_t *plugin, double sr, uint32_t min, uint32_t max) {
    (void)plugin; (void)sr; (void)min; (void)max;
    return true;
}

static void plugin_deactivate(const clap_plugin_t *plugin) {
    (void)plugin;
}

static bool plugin_start_processing(const clap_plugin_t *plugin) {
    (void)plugin;
    return true;
}

static void plugin_stop_processing(const clap_plugin_t *plugin) {
    (void)plugin;
}

static void plugin_reset(const clap_plugin_t *plugin) {
    (void)plugin;
}

static int32_t plugin_process(const clap_plugin_t *plugin, const clap_process_t *process) {
    (void)plugin;
    // CLAP convention: one bus with multiple channels.
    // audio_outputs_count = number of buses (1)
    // channel_count = number of channels per bus
    for (uint32_t bus = 0; bus < process->audio_outputs_count; bus++) {
        uint32_t out_ch = process->audio_outputs[bus].channel_count;
        uint32_t in_ch  = process->audio_inputs[bus].channel_count;
        for (uint32_t ch = 0; ch < out_ch; ch++) {
            uint32_t ich = ch < in_ch ? ch : 0;
            float *out = process->audio_outputs[bus].data32[ch];
            float *in  = process->audio_inputs[bus].data32[ich];
            memcpy(out, in, process->frames_count * sizeof(float));
        }
    }
    return 0; // CLAP_PROCESS_CONTINUE
}

static const void *plugin_get_extension(const clap_plugin_t *plugin, const char *id) {
    (void)plugin; (void)id;
    return NULL;
}

static void plugin_on_main_thread(const clap_plugin_t *plugin) {
    (void)plugin;
}

static const char *features[] = { "audio-effect", NULL };

static const clap_plugin_descriptor_t test_descriptor = {
    CLAP_VERSION_INIT,
    "com.maolan.test.passthrough",
    "Test Passthrough",
    "Maolan",
    "https://maolan.rs",
    "",
    "",
    "0.0.1",
    "Minimal passthrough plugin for host testing",
    features,
};

static clap_plugin_t test_plugin = {
    &test_descriptor,
    NULL,
    plugin_init,
    plugin_destroy,
    plugin_activate,
    plugin_deactivate,
    plugin_start_processing,
    plugin_stop_processing,
    plugin_reset,
    plugin_process,
    plugin_get_extension,
    plugin_on_main_thread,
};

/* === Factory === */

static uint32_t factory_count(const clap_plugin_factory_t *factory) {
    (void)factory;
    return 1;
}

static const clap_plugin_descriptor_t *factory_descriptor(const clap_plugin_factory_t *factory, uint32_t index) {
    (void)factory;
    return index == 0 ? &test_descriptor : NULL;
}

static const clap_plugin_t *factory_create(const clap_plugin_factory_t *factory, const clap_host_t *host, const char *plugin_id) {
    (void)factory; (void)host; (void)plugin_id;
    return &test_plugin;
}

static clap_plugin_factory_t test_factory = {
    factory_count,
    factory_descriptor,
    factory_create,
};

/* === Entry === */

static bool entry_init(const char *path) {
    (void)path;
    return true;
}

static void entry_deinit(void) {
}

static const void *entry_get_factory(const char *factory_id) {
    if (strcmp(factory_id, "clap.plugin-factory") == 0) {
        return &test_factory;
    }
    return NULL;
}

__attribute__((visibility("default"))) const clap_plugin_entry_t clap_entry = {
    CLAP_VERSION_INIT,
    entry_init,
    entry_deinit,
    entry_get_factory,
};
