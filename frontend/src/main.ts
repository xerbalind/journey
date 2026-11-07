import { Map, setWorkerUrl, AttributionControl, type IControl } from "maplibre-gl";
import workerUrl from "maplibre-gl/dist/maplibre-gl-worker.mjs?worker&url";
import "maplibre-gl/dist/maplibre-gl.css";

setWorkerUrl(workerUrl);

const apiUrl = import.meta.env.VITE_API_URL || "/api";
const response = await fetch(`${apiUrl}/position`);
const position = await response.json();

const map = new Map({
    container: 'map',
    style: 'https://tiles.openfreemap.org/styles/bright',
    center: position,
    zoom: 16,
    attributionControl: false
});

map.addControl(
    new AttributionControl({
        compact: true,
        customAttribution:
            '<a href="https://github.com/xerbalind/journey" target="_blank" rel="noopener noreferrer">Source Code</a>'
    }),
    "bottom-right"
);


/*
 * Source: https://maplibre.org/maplibre-gl-js/docs/examples/add-an-animated-icon-to-the-map/
 */
const size = 200;
export const pulsingDot: any = {
    width: size,
    height: size,
    data: new Uint8Array(size * size * 4),

    // get rendering context for the map canvas when layer is added to the map
    onAdd() {
        const canvas = document.createElement('canvas');
        canvas.width = this.width;
        canvas.height = this.height;
        this.context = canvas.getContext('2d');
    },

    // called once before every frame where the icon will be used
    render() {
        const duration = 1000;
        const t = (performance.now() % duration) / duration;

        const radius = (size / 2) * 0.3;
        const outerRadius = (size / 2) * 0.7 * t + radius;
        const context = this.context;

        // draw outer circle
        context.clearRect(0, 0, this.width, this.height);
        context.beginPath();
        context.arc(
            this.width / 2,
            this.height / 2,
            outerRadius,
            0,
            Math.PI * 2
        );
        context.fillStyle = `rgba(255, 127, 0,${1 - t})`;
        context.fill();

        // draw inner circle
        context.beginPath();
        context.arc(
            this.width / 2,
            this.height / 2,
            radius,
            0,
            Math.PI * 2
        );
        context.fillStyle = 'rgba(255, 127, 0, 1)';
        context.strokeStyle = 'white';
        context.lineWidth = 2 + 4 * (1 - t);
        context.fill();
        context.stroke();

        // update this image's data with data from the canvas
        this.data = context.getImageData(
            0,
            0,
            this.width,
            this.height
        ).data;

        // continuously repaint the map, resulting in the smooth animation of the dot
        map.triggerRepaint();

        // return `true` to let the map know that the image was updated
        return true;
    }
};

type Feature = {
    properties: { color: string },
    geometry: { type: string, coordinates: Array<[number, number]> }
};

class RouteToggleControl implements IControl {
    private container: HTMLDivElement | undefined;

    private pointCount: number;
    private routeCount: number;

    constructor(pointCount: number, routeCount: number) {
        this.pointCount = pointCount;
        this.routeCount = routeCount;
    }

    onAdd(map: Map): HTMLElement {
        const container = document.createElement('div');
        container.className = 'maplibregl-ctrl maplibregl-ctrl-group route-toggle';

        const addCheckbox = (text: string, prefix: string, count: number) => {
            const label = document.createElement('label');
            const checkbox = document.createElement('input');
            checkbox.type = 'checkbox';
            checkbox.checked = true;
            checkbox.addEventListener('change', () => {
                for (let index = 0; index < count; index++) {
                    map.setLayoutProperty(
                        `${prefix}-layer-${index}`,
                        'visibility',
                        checkbox.checked ? 'visible' : 'none'
                    );
                }
            });
            label.append(checkbox, text);
            container.appendChild(label);
        };

        addCheckbox('Predicted route', 'routes', this.routeCount);
        addCheckbox('GPS points', 'points', this.pointCount);

        this.container = container;
        return container;
    }

    onRemove(): void {
        this.container?.remove();
        this.container = undefined;
    }
}

map.on('load', async () => {
    // Get geojsons
    const geojsonResponse = await fetch(`${apiUrl}/journey.geojsons`);
    const geojsons: { points: Feature[], routes: Feature[] } = await geojsonResponse.json();

    // Render each day
    const addLines = (prefix: string, features: Feature[], visible: boolean) => {
        features.forEach((geojson, index) => {
            const sourceId = `${prefix}-${index}`;
            const layerId = `${prefix}-layer-${index}`;
            const color = geojson.properties.color;

            map.addSource(sourceId, {
                'type': 'geojson',
                'data': geojson
            });

            map.addLayer({
                'id': layerId,
                'type': 'line',
                'source': sourceId,
                'layout': {
                    'line-join': 'round',
                    'line-cap': 'round',
                    'visibility': visible ? 'visible' : 'none'
                },
                'paint': {
                    'line-color': color,
                    'line-width': 8
                }
            });
        });
    };

    // Raw GPS lines are added last so they render on top of the predicted route.
    addLines('routes', geojsons.routes, true);
    addLines('points', geojsons.points, true);

    map.addControl(
        new RouteToggleControl(geojsons.points.length, geojsons.routes.length),
        'top-right'
    );

    map.addImage('pulsing-dot', pulsingDot, { pixelRatio: 2 });
    map.addSource('position', {
        'type': 'geojson',
        'data': {
            'type': 'FeatureCollection',
            'features': [
                {
                    'type': 'Feature',
                    'geometry': {
                        'type': 'Point',
                        'coordinates': position
                    }
                }
            ]
        }
    });

    map.addLayer({
        'id': 'position',
        'type': 'symbol',
        'source': 'position',
        'layout': {
            'icon-image': 'pulsing-dot'
        }
    });
});
