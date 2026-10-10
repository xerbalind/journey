import { Map, setWorkerUrl } from "maplibre-gl";
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
    zoom: 16
});

/*
 * Source: https://maplibre.org/maplibre-gl-js/docs/examples/add-an-animated-icon-to-the-map/
 */
const size = 200;
export const pulsingDot: any = {
    width: size,
    height: size,
    data: new Uint8Array(size * size * 4),

    // get rendering context for the map canvas when layer is added to the map
    onAdd () {
        const canvas = document.createElement('canvas');
        canvas.width = this.width;
        canvas.height = this.height;
        this.context = canvas.getContext('2d');
    },

    // called once before every frame where the icon will be used
    render () {
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

map.on('load', () => {
    map.addSource('route', {
        'type': 'geojson',
	'data': `${apiUrl}/journey.geojson`,
    });
    map.addLayer({
        'id': 'route',
        'type': 'line',
        'source': 'route',
        'layout': {
            'line-join': 'round',
            'line-cap': 'round'
        },
        'paint': {
            'line-color': '#ff7f00',
            'line-width': 8
        }
    });

    
    map.addImage('pulsing-dot', pulsingDot, {pixelRatio: 2});
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
